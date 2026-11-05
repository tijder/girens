//! A small Plex client with exactly the functionality Girens needs.
//!
//! It replaces the `plexapi` Python library and `legacy/plex_compat.py`.
//! Everything is async and independent of GTK.

mod de;
mod error;
pub mod model;
pub mod myplex;
pub mod playback;
pub mod playqueue;
mod server;

pub use error::{Error, Result};
pub use server::{ClientInfo, Page, PlexServer};

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
    download_to_file_with_progress(url, path, |_, _| {}).await
}

/// Like [`download_to_file`]; `progress` is called with the bytes received so
/// far and, when the server tells, the total.
pub async fn download_to_file_with_progress(
    url: &str,
    path: &std::path::Path,
    progress: impl Fn(u64, Option<u64>) + Send,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let io_error = |path: &std::path::Path| {
        let path = path.display().to_string();
        move |source| Error::Io { path, source }
    };
    let mut response = reqwest::Client::new().get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status {
            status,
            url: "download".into(),
        });
    }
    let total = response.content_length();

    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(io_error(dir))?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(".tmp");
    let partial = std::path::PathBuf::from(partial);
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(io_error(&partial))?;
    let written = async {
        let mut received = 0u64;
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
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(err);
    }
    tokio::fs::rename(&partial, path)
        .await
        .map_err(io_error(path))
}
