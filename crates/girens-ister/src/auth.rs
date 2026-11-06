//! Finding a server and signing in to it.
//!
//! An Ister server leaves passwords to an OpenID Connect provider: the user
//! signs in there, in the browser, and the provider sends the browser back to
//! a port this program listens on.

use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use girens_model::{Error, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// The client every Ister player signs in as; it has no secret.
const CLIENT_ID: &str = "ister";
/// A token is replaced when it has less than this left.
const EXPIRY_MARGIN: Duration = Duration::from_secs(30);

/// What a server tells about itself at `/.well-known/ister`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WellKnown {
    pub name: String,
    /// The discovery document of the provider users sign in at.
    pub openid_url: String,
    /// Where the API is, which may be below the address the user typed.
    pub api_url: String,
}

/// `http` for this computer and bare addresses, `https` for anything else,
/// unless the address says which.
fn with_scheme(address: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.contains("://") {
        return address.to_owned();
    }
    let host = address.split(['/', ':']).next().unwrap_or_default();
    let local = host == "localhost" || host.parse::<std::net::Ipv4Addr>().is_ok();
    format!("{}://{address}", if local { "http" } else { "https" })
}

/// Asks the server at `address` (as the user typed it) who it is.
pub async fn discover(http: &reqwest::Client, address: &str) -> Result<WellKnown> {
    let url = format!("{}/.well-known/ister", with_scheme(address));
    let response = http.get(&url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status { status, url });
    }
    let text = response.text().await?;
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    match lines.as_slice() {
        [name, openid_url, api_url] if openid_url.starts_with("http") => Ok(WellKnown {
            name: (*name).to_owned(),
            openid_url: (*openid_url).to_owned(),
            api_url: api_url.trim_end_matches('/').to_owned(),
        }),
        _ => Err(Error::Server(format!("{url} is not an Ister server"))),
    }
}

/// The addresses of the provider that matter here.
#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
}

impl Provider {
    pub async fn fetch(http: &reqwest::Client, openid_url: &str) -> Result<Self> {
        let response = http.get(openid_url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status,
                url: openid_url.to_owned(),
            });
        }
        let text = response.text().await?;
        serde_json::from_str(&text).map_err(|source| Error::Decode {
            url: openid_url.to_owned(),
            source,
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// What proves to the server who is asking.
#[derive(Clone)]
pub struct Tokens {
    pub access: String,
    /// Gets a new access token when that has expired. The provider may hand
    /// out a new one with every use, which then replaces this one.
    pub refresh: String,
    expires: Instant,
}

impl Tokens {
    fn new(response: TokenResponse, previous_refresh: Option<&str>) -> Result<Self> {
        let refresh = response
            .refresh_token
            .or_else(|| previous_refresh.map(str::to_owned))
            .ok_or_else(|| Error::Server("the sign-in gave no refresh token".into()))?;
        Ok(Self {
            access: response.access_token,
            refresh,
            expires: Instant::now() + Duration::from_secs(response.expires_in.unwrap_or(60)),
        })
    }

    pub fn is_fresh(&self) -> bool {
        Instant::now() + EXPIRY_MARGIN < self.expires
    }
}

async fn request_tokens(
    http: &reqwest::Client,
    provider: &Provider,
    form: &[(&str, &str)],
    previous_refresh: Option<&str>,
) -> Result<Tokens> {
    let response = http
        .post(&provider.token_endpoint)
        .form(form)
        .send()
        .await?;
    let status = response.status();
    // The provider refuses a code or refresh token it no longer accepts
    // with 400 `invalid_grant`.
    if status == reqwest::StatusCode::BAD_REQUEST || status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::Unauthorized);
    }
    if !status.is_success() {
        return Err(Error::Status {
            status,
            url: provider.token_endpoint.clone(),
        });
    }
    let text = response.text().await?;
    let response = serde_json::from_str(&text).map_err(|source| Error::Decode {
        url: provider.token_endpoint.clone(),
        source,
    })?;
    Tokens::new(response, previous_refresh)
}

/// New tokens for a refresh token; [`Error::Unauthorized`] when the provider
/// no longer accepts it, which means signing in again.
pub async fn refresh(http: &reqwest::Client, provider: &Provider, refresh: &str) -> Result<Tokens> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", CLIENT_ID),
        ("refresh_token", refresh),
    ];
    request_tokens(http, provider, &form, Some(refresh)).await
}

/// Makes the provider forget a refresh token, when signing out.
pub async fn revoke(http: &reqwest::Client, provider: &Provider, refresh: &str) -> Result<()> {
    let Some(endpoint) = &provider.revocation_endpoint else {
        return Ok(());
    };
    let form = [
        ("client_id", CLIENT_ID),
        ("token", refresh),
        ("token_type_hint", "refresh_token"),
    ];
    http.post(endpoint).form(&form).send().await?;
    Ok(())
}

fn random_text(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::getrandom(&mut buffer)
        .map_err(|err| Error::Server(format!("no random numbers: {err}")))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// A sign-in under way: the user has to open [`Login::url`] in a browser,
/// and [`Login::finish`] waits for the browser to come back.
pub struct Login {
    /// The page to sign in at.
    pub url: String,
    provider: Provider,
    listener: TcpListener,
    redirect: String,
    state: String,
    verifier: String,
}

impl Login {
    /// Prepares a sign-in. With `ask_again` the provider asks who it is
    /// even when the browser is still signed in, to add another user.
    pub async fn start(provider: Provider, ask_again: bool) -> Result<Self> {
        let io = |source| Error::Io {
            path: "localhost".into(),
            source,
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(io)?;
        let port = listener.local_addr().map_err(io)?.port();
        // The form the other players use, which is what providers are set
        // up to allow.
        let redirect = format!("http://localhost:{port}");
        let state = random_text(16)?;
        let verifier = random_text(48)?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

        let mut url = Url::parse(&provider.authorization_endpoint)?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", CLIENT_ID)
                .append_pair("redirect_uri", &redirect)
                // As the other players ask: what a provider set up for Ister
                // is sure to grant.
                .append_pair("scope", "openid")
                .append_pair("state", &state)
                .append_pair("code_challenge", &challenge)
                .append_pair("code_challenge_method", "S256");
            if ask_again {
                query.append_pair("prompt", "login");
            }
        }
        Ok(Self {
            url: url.into(),
            provider,
            listener,
            redirect,
            state,
            verifier,
        })
    }

    /// Waits for the browser to return from the provider and trades what it
    /// brings for tokens. Dropping the future gives up the sign-in.
    pub async fn finish(self, http: &reqwest::Client) -> Result<Tokens> {
        let code = loop {
            if let Some(answer) = self.next_answer().await? {
                break answer?;
            }
        };
        let form = [
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("code", code.as_str()),
            ("redirect_uri", self.redirect.as_str()),
            ("code_verifier", self.verifier.as_str()),
        ];
        request_tokens(http, &self.provider, &form, None).await
    }

    /// Handles one request of the browser. `None` when it was not the
    /// answer of the provider (browsers also ask for an icon).
    async fn next_answer(&self) -> Result<Option<Result<String>>> {
        let io = |source| Error::Io {
            path: self.redirect.clone(),
            source,
        };
        let (mut stream, _) = self.listener.accept().await.map_err(io)?;
        let mut buffer = vec![0u8; 8192];
        let mut length = 0;
        // The first line holds everything that matters.
        while length < buffer.len() && !buffer[..length].contains(&b'\n') {
            match stream.read(&mut buffer[length..]).await {
                Ok(0) | Err(_) => break,
                Ok(read) => length += read,
            }
        }
        let request = String::from_utf8_lossy(&buffer[..length]);
        let target = request.split_whitespace().nth(1).unwrap_or_default();
        let answer = Url::parse(&format!("{}{target}", self.redirect))
            .ok()
            .and_then(|url| self.read_answer(&url));
        let (status, text) = match &answer {
            Some(Ok(_)) => ("200 OK", "You are signed in. You can close this page and return to Girens."),
            Some(Err(_)) => ("400 Bad Request", "Signing in did not work. Return to Girens to try again."),
            None => ("404 Not Found", ""),
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>Girens</title>\
             <body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\">{text}"
        );
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        Ok(answer)
    }

    fn read_answer(&self, url: &Url) -> Option<Result<String>> {
        let value = |name: &str| {
            url.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        if let Some(error) = value("error") {
            let detail = value("error_description").unwrap_or(error);
            return Some(Err(Error::Server(detail)));
        }
        let code = value("code")?;
        // Only the answer to the question that was asked counts.
        if value("state").as_deref() != Some(&self.state) {
            return Some(Err(Error::Unauthorized));
        }
        Some(Ok(code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scheme_follows_the_address() {
        assert_eq!(with_scheme("localhost:8080"), "http://localhost:8080");
        assert_eq!(with_scheme("192.168.1.5/api/"), "http://192.168.1.5/api");
        assert_eq!(with_scheme("media.example.org"), "https://media.example.org");
        assert_eq!(with_scheme(" http://nas.lan/ "), "http://nas.lan");
    }
}
