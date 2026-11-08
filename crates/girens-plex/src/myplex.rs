//! plex.tv: signing in, the profiles of a Plex Home and the servers of an
//! account.

use reqwest::StatusCode;
use serde::Deserialize;

use crate::server::{send, ClientInfo};
use crate::{Error, PlexServer, Result};

const PLEX_TV: &str = "https://plex.tv";
const SIGN_IN: &str = "https://plex.tv/api/v2/users/signin";
const USER: &str = "/api/v2/user";
const PINS: &str = "https://plex.tv/api/v2/pins";
const AUTH_PAGE: &str = "https://app.plex.tv/auth";
/// Where the user can type the code of a [`Pin`] instead of opening its
/// [`Pin::auth_url`].
pub const LINK_PAGE: &str = "https://plex.tv/link";
const RESOURCES: &str = "/api/v2/resources?includeHttps=1&includeRelay=1";
const HOME_USERS: &str = "/api/v2/home/users";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub auth_token: String,
    pub uuid: String,
    /// Managed profiles of a Plex Home have neither a username nor an
    /// e-mail address.
    pub username: Option<String>,
    pub email: Option<String>,
    /// The name to show.
    pub title: Option<String>,
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

/// A profile of the Plex Home the account belongs to. An account without a
/// Home has one: itself.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HomeUser {
    pub uuid: String,
    /// The name to show.
    #[serde(default)]
    pub title: String,
    /// Avatar URL.
    pub thumb: Option<String>,
    /// Switching to this profile takes a PIN.
    #[serde(default)]
    pub protected: bool,
    /// A managed profile, e.g. of a child: it sees part of the libraries.
    #[serde(default)]
    pub restricted: bool,
    #[serde(default)]
    pub admin: bool,
}

#[derive(Deserialize)]
struct Home {
    #[serde(default)]
    users: Vec<HomeUser>,
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
/// with two-factor authentication. The [code](Self::code) is short enough to
/// type at [`LINK_PAGE`] on another device, for when the browser is not on
/// this one.
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
    /// `https://plex.tv`, or a stand-in under test.
    plex_tv: String,
}

impl MyPlex {
    /// Signs in with a username or e-mail address and a password. An account
    /// with two-factor authentication also needs the `verification_code`
    /// of its authenticator; without it plex.tv answers as to a wrong
    /// password.
    pub async fn sign_in(
        login: &str,
        password: &str,
        verification_code: Option<&str>,
        client: &ClientInfo,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(None))
            .build()?;
        let body = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("login", login)
                .append_pair("password", password)
                .append_pair("rememberMe", "true");
            if let Some(code) = verification_code.filter(|code| !code.is_empty()) {
                form.append_pair("verificationCode", code);
            }
            form.finish()
        };
        let account: Account = send(
            http.post(SIGN_IN)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body),
        )
        .await?;
        Self::with_account(account, client, PLEX_TV)
    }

    /// Starts a sign-in through the browser. Open [`Pin::auth_url`], or have
    /// the user type [`Pin::code`] at [`LINK_PAGE`], and call
    /// [`Self::check_pin`] until it returns a session. The pin is kept for
    /// about fifteen minutes.
    pub async fn create_pin(client: &ClientInfo) -> Result<Pin> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(None))
            .build()?;
        // Not a strong pin: those are too long to type at the link page.
        send(http.post(format!("{PINS}?strong=false"))).await
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
        Self::with_token_at(PLEX_TV, token, client).await
    }

    /// [`Self::with_token`] against another address than plex.tv, for tests.
    #[doc(hidden)]
    pub async fn with_token_at(plex_tv: &str, token: &str, client: &ClientInfo) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(Some(token)))
            .build()?;
        let account: Account = send(http.get(format!("{plex_tv}{USER}"))).await?;
        Self::with_account(account, client, plex_tv)
    }

    fn with_account(account: Account, client: &ClientInfo, plex_tv: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(Some(&account.auth_token)))
            .build()?;
        Ok(Self {
            http,
            client: client.clone(),
            account,
            plex_tv: plex_tv.to_owned(),
        })
    }

    pub fn account(&self) -> &Account {
        &self.account
    }

    /// The servers this account can use.
    pub async fn servers(&self) -> Result<Vec<Resource>> {
        let url = format!("{}{RESOURCES}", self.plex_tv);
        let resources: Vec<Resource> = send(self.http.get(url)).await?;
        Ok(resources.into_iter().filter(Resource::is_server).collect())
    }

    /// The profiles of the Plex Home of this account.
    pub async fn home_users(&self) -> Result<Vec<HomeUser>> {
        let url = format!("{}{HOME_USERS}", self.plex_tv);
        let home: Home = send(self.http.get(url)).await?;
        Ok(home.users)
    }

    /// A session as another profile of the Home. `pin` is needed when the
    /// profile is [protected](HomeUser::protected); a wrong one gives
    /// [`Error::WrongPin`]. This session stays valid.
    pub async fn switch_user(&self, user_uuid: &str, pin: Option<&str>) -> Result<Self> {
        let address = format!("{}{HOME_USERS}/{user_uuid}/switch", self.plex_tv);
        let mut url = url::Url::parse(&address)?;
        if let Some(pin) = pin {
            url.query_pairs_mut().append_pair("pin", pin);
        }
        let response = self.http.post(url).send().await?;
        let status = response.status();
        // plex.tv refuses a wrong PIN with one of these; none of them means
        // that the session itself is no longer valid, which is found out
        // when the account is restored.
        let refused = [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::BAD_REQUEST,
            StatusCode::UNPROCESSABLE_ENTITY,
        ];
        if refused.contains(&status) {
            return Err(Error::WrongPin);
        }
        if !status.is_success() {
            // Without the PIN, which does not belong in a log.
            return Err(Error::Status {
                status,
                url: address,
            });
        }
        let body = response.bytes().await?;
        let account: Account = serde_json::from_slice(&body).map_err(|source| Error::Decode {
            url: address,
            source,
        })?;
        Self::with_account(account, &self.client, &self.plex_tv)
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
