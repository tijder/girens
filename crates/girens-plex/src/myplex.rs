//! plex.tv: signing in and finding the servers of an account.

use serde::Deserialize;

use crate::server::{send, ClientInfo};
use crate::{Error, PlexServer, Result};

const SIGN_IN: &str = "https://plex.tv/api/v2/users/signin";
const USER: &str = "https://plex.tv/api/v2/user";
const PINS: &str = "https://plex.tv/api/v2/pins";
const AUTH_PAGE: &str = "https://app.plex.tv/auth";
const RESOURCES: &str = "https://plex.tv/api/v2/resources?includeHttps=1&includeRelay=1";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub auth_token: String,
    pub uuid: String,
    pub username: Option<String>,
    pub email: Option<String>,
    /// Avatar URL.
    pub thumb: Option<String>,
    pub subscription: Option<Subscription>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    #[serde(default)]
    pub active: bool,
    pub plan: Option<String>,
}

/// A device registered with the account; servers have `provides == "server"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub name: String,
    pub client_identifier: String,
    #[serde(default)]
    pub provides: String,
    #[serde(default)]
    pub owned: bool,
    /// Token for this server; differs from the account token for shared servers.
    pub access_token: Option<String>,
    #[serde(default)]
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Connection {
    pub uri: String,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub relay: bool,
}

impl Resource {
    pub fn is_server(&self) -> bool {
        self.provides
            .split(',')
            .any(|provided| provided == "server")
    }

    /// Connections in the order to try them: local, then remote, then relay.
    /// For servers of other people only non-local connections are usable.
    pub fn preferred_connections(&self) -> Vec<&Connection> {
        let rank = |connection: &Connection| match (connection.relay, connection.local) {
            (true, _) => 2,
            (false, true) => 0,
            (false, false) => 1,
        };
        let mut connections: Vec<&Connection> = self
            .connections
            .iter()
            .filter(|connection| self.owned || !connection.local)
            .collect();
        connections.sort_by_key(|connection| rank(connection));
        connections
    }
}

/// A pending sign-in through the browser: the user approves it on plex.tv,
/// after which the pin yields an account token. This also works for accounts
/// with two-factor authentication.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    pub id: u64,
    pub code: String,
    /// Set once the user has approved the sign-in.
    pub auth_token: Option<String>,
}

impl Pin {
    /// The page to open in the browser.
    pub fn auth_url(&self, client: &ClientInfo) -> String {
        // Plex expects the parameters after the `#`, not as a query string.
        let parameters = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("clientID", &client.identifier)
            .append_pair("code", &self.code)
            .append_pair("context[device][product]", &client.product)
            .finish();
        format!("{AUTH_PAGE}#?{parameters}")
    }
}

/// A plex.tv session.
#[derive(Debug, Clone)]
pub struct MyPlex {
    http: reqwest::Client,
    client: ClientInfo,
    account: Account,
}

impl MyPlex {
    /// Signs in with a username or e-mail address and a password.
    pub async fn sign_in(login: &str, password: &str, client: &ClientInfo) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(None))
            .build()?;
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("login", login)
            .append_pair("password", password)
            .append_pair("rememberMe", "true")
            .finish();
        let account: Account = send(
            http.post(SIGN_IN)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body),
        )
        .await?;
        Self::with_account(account, client)
    }

    /// Starts a sign-in through the browser. Open [`Pin::auth_url`] and call
    /// [`Self::check_pin`] until it returns a session.
    pub async fn create_pin(client: &ClientInfo) -> Result<Pin> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(None))
            .build()?;
        send(http.post(format!("{PINS}?strong=true"))).await
    }

    /// The session, once the user has approved `pin` in the browser.
    pub async fn check_pin(pin: &Pin, client: &ClientInfo) -> Result<Option<Self>> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(None))
            .build()?;
        let pin: Pin = send(http.get(format!("{PINS}/{}", pin.id))).await?;
        match pin.auth_token {
            Some(token) if !token.is_empty() => Ok(Some(Self::with_token(&token, client).await?)),
            _ => Ok(None),
        }
    }

    /// Restores a session from a stored account token.
    pub async fn with_token(token: &str, client: &ClientInfo) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(Some(token)))
            .build()?;
        let account: Account = send(http.get(USER)).await?;
        Self::with_account(account, client)
    }

    fn with_account(account: Account, client: &ClientInfo) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(Some(&account.auth_token)))
            .build()?;
        Ok(Self {
            http,
            client: client.clone(),
            account,
        })
    }

    pub fn account(&self) -> &Account {
        &self.account
    }

    /// The servers this account can use.
    pub async fn servers(&self) -> Result<Vec<Resource>> {
        let resources: Vec<Resource> = send(self.http.get(RESOURCES)).await?;
        Ok(resources.into_iter().filter(Resource::is_server).collect())
    }

    /// Tries the connections of `resource` in order of preference and returns
    /// the first that works.
    pub async fn connect(&self, resource: &Resource) -> Result<PlexServer> {
        let token = resource
            .access_token
            .as_deref()
            .unwrap_or(&self.account.auth_token);
        let mut last_error = None;
        for connection in resource.preferred_connections() {
            match PlexServer::connect(&connection.uri, token, &self.client).await {
                Ok(server) => return Ok(server),
                Err(Error::Unauthorized) => return Err(Error::Unauthorized),
                Err(error) => last_error = Some(error),
            }
        }
        match last_error {
            Some(error) if error.is_network() => Err(Error::NoConnection(resource.name.clone())),
            Some(error) => Err(error),
            None => Err(Error::NoConnection(resource.name.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_page_gets_its_parameters_after_the_hash() {
        let pin = Pin {
            id: 1,
            code: "abc123".into(),
            auth_token: None,
        };
        let client = ClientInfo {
            identifier: "client id".into(),
            product: "Girens".into(),
            version: "1".into(),
            device_name: "test".into(),
        };
        assert_eq!(
            pin.auth_url(&client),
            "https://app.plex.tv/auth#?clientID=client+id&code=abc123&context%5Bdevice%5D%5Bproduct%5D=Girens"
        );
    }

    #[test]
    fn servers_are_tried_local_first_then_remote_then_relay() {
        let connection = |uri: &str, local, relay| Connection {
            uri: uri.into(),
            local,
            relay,
        };
        let mut resource = Resource {
            name: "server".into(),
            client_identifier: "id".into(),
            provides: "client,server".into(),
            owned: true,
            access_token: None,
            connections: vec![
                connection("relay", false, true),
                connection("remote", false, false),
                connection("local", true, false),
            ],
        };
        assert!(resource.is_server());
        let order: Vec<&str> = resource
            .preferred_connections()
            .iter()
            .map(|connection| connection.uri.as_str())
            .collect();
        assert_eq!(order, ["local", "remote", "relay"]);

        // Someone else's server is not on our network.
        resource.owned = false;
        let order: Vec<&str> = resource
            .preferred_connections()
            .iter()
            .map(|connection| connection.uri.as_str())
            .collect();
        assert_eq!(order, ["remote", "relay"]);
    }
}
