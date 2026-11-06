//! A small Plex client with exactly the functionality Girens needs.
//!
//! Everything is async and independent of GTK.

pub use girens_model::model;
pub mod myplex;
pub mod playback;
pub mod playqueue;
mod server;

pub use girens_model::{Error, ItemId, Page, Result};
pub use server::{ClientInfo, PlexServer};

/// Downloads `url` (an image URL from [`PlexServer::image_url`] or an avatar).
pub async fn download(url: &str) -> Result<Vec<u8>> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let response = CLIENT
        .get_or_init(reqwest::Client::new)
        .get(url)
        .send()
        .await?;
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

/// Like [`download_to_file`]; `progress` is called with the bytes received so
/// far and, when the server tells, the total.
///
/// With `resume`, a `<path>.tmp` left by an earlier attempt is continued
/// where it stopped (if the server supports that), and it is kept when this
/// attempt fails too.
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
    let mut partial = path.as_os_str().to_owned();
    partial.push(".tmp");
    let partial = std::path::PathBuf::from(partial);
    let existing = match (resume, tokio::fs::metadata(&partial).await) {
        (true, Ok(metadata)) => metadata.len(),
        _ => 0,
    };

    let mut request = reqwest::Client::new().get(url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let mut response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status {
            status,
            url: "download".into(),
        });
    }
    // 206 means the server continues after what is already there; anything
    // else is the whole file again.
    let continued = existing > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut received = if continued { existing } else { 0 };
    let total = response.content_length().map(|length| received + length);

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
    tokio::fs::rename(&partial, path)
        .await
        .map_err(io_error(path))
}
