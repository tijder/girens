use reqwest::StatusCode;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The token is missing, wrong or no longer valid.
    #[error("the Plex server rejected the credentials")]
    Unauthorized,
    #[error("{url} answered with HTTP {status}")]
    Status { status: StatusCode, url: String },
    /// The server could not be reached or the connection broke.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("unexpected response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("no item with rating key {0}")]
    NotFound(u64),
    #[error("could not write {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// None of the connections plex.tv lists for a server could be used.
    #[error("could not connect to {0}")]
    NoConnection(String),
}

impl Error {
    /// Whether retrying later could help (as opposed to logging in again).
    pub fn is_network(&self) -> bool {
        matches!(self, Error::Network(_))
    }
}
