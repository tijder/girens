//! Fetching files over HTTP.

use std::time::Duration;

use crate::{Error, Result};

/// How long a server gets to accept a connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a server may stay silent in the middle of an answer. Not a limit
/// on the whole answer: a download takes as long as it takes.
pub const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// How often a connection that is not in use is checked to be still there.
const KEEPALIVE: Duration = Duration::from_secs(30);

/// What every HTTP client of Girens starts from: without these limits a
/// server that accepts a connection and then says nothing is waited for
/// forever.
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        // A connection that is kept for the next request can die unnoticed
        // (a router that forgot it, a server that restarted): asking after
        // it finds out before a request is sent into the void.
        .tcp_keepalive(KEEPALIVE)
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| client_builder().build().unwrap_or_default())
}

/// Downloads `url` (an image or an avatar).
pub async fn download(url: &str) -> Result<Vec<u8>> {
    let response = client().get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        // The URL may carry a token, so it is left out of the error.
        return Err(Error::Status {
            status,
            url: "download".into(),
        });
    }
    Ok(response.bytes().await?.to_vec())
}

/// Downloads `url` into `path` without holding the file in memory. The data
/// is written to `<path>.tmp` first, so `path` only ever holds a complete
/// download.
pub async fn download_to_file(url: &str, path: &std::path::Path) -> Result<()> {
    download_to_file_with_progress(url, path, false, |_, _| {}).await
}

fn beside(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

/// What tells one version of a file on the server from the next, for
/// `If-Range`: a strong `ETag`, or else when it was last changed.
fn validator(response: &reqwest::Response) -> Option<String> {
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    header(reqwest::header::ETAG)
        .filter(|tag| !tag.starts_with("W/"))
        .or_else(|| header(reqwest::header::LAST_MODIFIED))
}

/// The size in `Content-Range: bytes */<size>`, which comes with a 416.
fn whole_size(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit('/')
        .next()?
        .trim()
        .parse()
        .ok()
}

/// Like [`download_to_file`]; `progress` is called with the bytes received so
/// far and, when the server tells, the total.
///
/// With `resume`, a `<path>.tmp` left by an earlier attempt is continued
/// where it stopped, and it is kept when this attempt fails too. What tells
/// that the file on the server is still the same one is kept beside it in
/// `<path>.tmp.etag`; when the server gave nothing of the kind, or the file
/// changed since, the download starts over.
pub async fn download_to_file_with_progress(
    url: &str,
    path: &std::path::Path,
    resume: bool,
    progress: impl Fn(u64, Option<u64>) + Send,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let io_error = |path: &std::path::Path| {
        let path = path.display().to_string();
        move |source| Error::Io { path, source }
    };
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(io_error(dir))?;
    }
    let partial = beside(path, ".tmp");
    let version_file = beside(path, ".tmp.etag");
    let mut existing = match (resume, tokio::fs::metadata(&partial).await) {
        (true, Ok(metadata)) => metadata.len(),
        _ => 0,
    };
    let version = match existing {
        0 => None,
        _ => tokio::fs::read_to_string(&version_file)
            .await
            .ok()
            .filter(|version| !version.trim().is_empty()),
    };
    if version.is_none() {
        // Nothing tells whether what is there is a part of this file.
        existing = 0;
    }

    let mut response = loop {
        let mut request = client().get(url);
        if let (true, Some(version)) = (existing > 0, &version) {
            request = request
                .header(reqwest::header::RANGE, format!("bytes={existing}-"))
                .header(reqwest::header::IF_RANGE, version.trim());
        }
        let response = request.send().await?;
        let status = response.status();
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && existing > 0 {
            if whole_size(&response) == Some(existing) {
                // Everything was there already; only the last step was
                // missing.
                let _ = tokio::fs::remove_file(&version_file).await;
                progress(existing, Some(existing));
                return tokio::fs::rename(&partial, path)
                    .await
                    .map_err(io_error(path));
            }
            // More than the file holds: not a part of it. Once more, whole.
            existing = 0;
            continue;
        }
        if !status.is_success() {
            return Err(Error::Status {
                status,
                url: "download".into(),
            });
        }
        break response;
    };
    // 206 means the server continues after what is already there; anything
    // else is the whole file again.
    let continued = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut received = if continued { existing } else { 0 };
    let total = response.content_length().map(|length| received + length);

    if resume {
        // Before the first byte, so that what is written can be continued.
        match validator(&response) {
            Some(version) => tokio::fs::write(&version_file, version)
                .await
                .map_err(io_error(&version_file))?,
            None => {
                let _ = tokio::fs::remove_file(&version_file).await;
            }
        }
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(continued)
        .truncate(!continued)
        .open(&partial)
        .await
        .map_err(io_error(&partial))?;
    let written = async {
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await.map_err(io_error(&partial))?;
            received += chunk.len() as u64;
            progress(received, total);
        }
        file.flush().await.map_err(io_error(&partial))
    }
    .await;
    drop(file);
    if let Err(err) = written {
        if !resume {
            let _ = tokio::fs::remove_file(&partial).await;
        }
        return Err(err);
    }
    let _ = tokio::fs::remove_file(&version_file).await;
    tokio::fs::rename(&partial, path)
        .await
        .map_err(io_error(path))
}
