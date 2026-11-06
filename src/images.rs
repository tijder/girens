//! Cover images: downloaded once into the cache directory the Python version
//! used (`$XDG_CACHE_HOME/<server id>/thumb_<key>`), decoded off the main
//! thread and kept in a small in-memory cache.

use girens_plex::ItemId;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use girens_plex::PlexServer;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use tokio::sync::Semaphore;

use crate::runtime;

/// Size requested from the server at scale 1; matches the Python version so
/// that its cached files stay valid.
const WIDTH: u32 = 200;
const HEIGHT: u32 = 300;
const MAX_DOWNLOADS: usize = 6;
/// Decoded images kept in memory, in bytes of pixel data.
const MEMORY_CACHE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
struct MemoryCache {
    textures: HashMap<PathBuf, gdk::Texture>,
    order: VecDeque<PathBuf>,
    bytes: usize,
}

thread_local! {
    static MEMORY: RefCell<MemoryCache> = RefCell::default();
}

fn size_of(texture: &gdk::Texture) -> usize {
    texture.width().max(0) as usize * texture.height().max(0) as usize * 4
}

fn remember(path: &Path, texture: &gdk::Texture) {
    MEMORY.with_borrow_mut(|cache| {
        if cache
            .textures
            .insert(path.to_path_buf(), texture.clone())
            .is_none()
        {
            cache.order.push_back(path.to_path_buf());
            cache.bytes += size_of(texture);
        }
        // The oldest images go first; widgets that still show one keep it
        // alive themselves.
        while cache.bytes > MEMORY_CACHE_BYTES && cache.order.len() > 1 {
            let Some(oldest) = cache.order.pop_front() else {
                break;
            };
            if let Some(texture) = cache.textures.remove(&oldest) {
                cache.bytes = cache.bytes.saturating_sub(size_of(&texture));
            }
        }
    });
}

/// 2 on a HiDPI screen, where images at scale 1 would look blurry.
fn scale() -> u32 {
    let Some(display) = gdk::Display::default() else {
        return 1;
    };
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|index| monitors.item(index).and_downcast::<gdk::Monitor>())
        .map(|monitor| monitor.scale_factor())
        .max()
        .unwrap_or(1)
        .clamp(1, 2) as u32
}

/// Directory with everything cached for `server`.
pub fn cache_dir(server: &PlexServer) -> PathBuf {
    cache_dir_of(server.machine_identifier())
}

/// The same directory, by the identifier of the server.
pub fn cache_dir_of(machine_identifier: &str) -> PathBuf {
    glib::user_cache_dir().join(machine_identifier)
}

/// A cover that was downloaded earlier, for when the server is out of reach.
pub async fn cached(machine_identifier: &str, key: ItemId) -> Option<gdk::Texture> {
    let dir = cache_dir_of(machine_identifier);
    let file = gio::spawn_blocking(move || {
        [2, 1]
            .into_iter()
            .map(|scale| dir.join(file_name(key, scale)))
            .find(|path| path.exists())
            .and_then(|path| gdk::Texture::from_filename(path).ok())
    })
    .await;
    file.ok().flatten()
}

/// File name of a cached cover. Scale 1 keeps the name the Python version
/// used; sharper versions get a suffix.
fn file_name(key: ItemId, scale: u32) -> String {
    match scale {
        1 => format!("thumb_{key}"),
        scale => format!("thumb_{key}@{scale}"),
    }
}

/// A cached cover of the item, whatever its resolution; for other programs
/// such as the desktop's media controls.
pub fn cached_cover(server: &PlexServer, key: ItemId) -> Option<PathBuf> {
    let dir = cache_dir(server);
    [2, 1]
        .into_iter()
        .map(|scale| dir.join(file_name(key, scale)))
        .find(|path| path.exists())
}

/// The image `thumb` (an item's `thumb`, `composite`, …), cached under `key`.
/// Returns `None` when it cannot be loaded; the reason is logged.
pub async fn cover(server: &PlexServer, key: ItemId, thumb: &str) -> Option<gdk::Texture> {
    let scale = scale();
    let path = cache_dir(server).join(file_name(key, scale));
    if let Some(texture) = MEMORY.with_borrow(|cache| cache.textures.get(&path).cloned()) {
        return Some(texture);
    }

    let url = server
        .image_url(thumb, WIDTH * scale, HEIGHT * scale)
        .to_string();
    let file = path.clone();
    // Checking the disk, downloading and decoding all happen off the main
    // thread.
    let loaded = runtime::run(async move {
        if !tokio::fs::try_exists(&file).await.unwrap_or(false) {
            static LIMIT: OnceLock<Semaphore> = OnceLock::new();
            let _permit = LIMIT
                .get_or_init(|| Semaphore::new(MAX_DOWNLOADS))
                .acquire()
                .await
                .map_err(|err| err.to_string())?;
            let bytes = girens_plex::download(&url)
                .await
                .map_err(|err| err.to_string())?;
            // Write next to the target and rename, so that a partial download
            // is never mistaken for a cached image.
            let partial = file.with_extension("part");
            if let Some(dir) = file.parent() {
                tokio::fs::create_dir_all(dir)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            tokio::fs::write(&partial, bytes)
                .await
                .map_err(|err| err.to_string())?;
            tokio::fs::rename(&partial, &file)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok::<_, String>(())
    })
    .await;
    if let Err(err) = loaded {
        eprintln!("Could not download the image for {key}: {err}");
        return None;
    }

    let file = path.clone();
    let decoded = gio::spawn_blocking(move || {
        let texture = gdk::Texture::from_filename(&file);
        if texture.is_err() {
            // Do not keep a file that cannot be shown.
            let _ = std::fs::remove_file(&file);
        }
        texture
    })
    .await;
    match decoded {
        Ok(Ok(texture)) => {
            remember(&path, &texture);
            Some(texture)
        }
        Ok(Err(err)) => {
            eprintln!("Could not decode {}: {err}", path.display());
            None
        }
        Err(_) => None,
    }
}
