//! Cover images: downloaded once into the cache directory of the server
//! (`$XDG_CACHE_HOME/<server id>/thumb_<key>~<picture>`), decoded off the
//! main thread and kept in a small in-memory cache.
//!
//! `<picture>` stands for the address the server gave the picture: when an
//! item gets another poster, that is another file. Covers nobody looked at
//! for a long time are removed by [`tidy`].

use girens_model::ItemId;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use crate::backend::Server;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use tokio::sync::Semaphore;

use crate::runtime;
use crate::util::{id_file_name, id_from_file_name};

/// Size requested from the server at scale 1.
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

/// A cover that is not shown for this long is removed from the disk.
const UNUSED_AFTER: Duration = Duration::from_secs(180 * 24 * 60 * 60);
/// How old the date of a file may get before showing it renews that date;
/// not every time, to spare the disk.
const RENEW_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

thread_local! {
    static MEMORY: RefCell<MemoryCache> = RefCell::default();
    /// Where the cover that was shown last for an item is, by the folder of
    /// its server and its key.
    static SHOWN: RefCell<HashMap<(PathBuf, ItemId), PathBuf>> = RefCell::default();
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
pub fn cache_dir(server: &Server) -> PathBuf {
    cache_dir_of(server.machine_identifier())
}

/// The same directory, by the identifier of the server.
pub fn cache_dir_of(machine_identifier: &str) -> PathBuf {
    glib::user_cache_dir().join(machine_identifier)
}

/// Whether `time` is longer ago than `age`.
fn older_than(time: std::io::Result<SystemTime>, age: Duration) -> bool {
    time.ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|elapsed| elapsed > age)
}

/// The start of the name of every cover of an item.
fn name_start(key: ItemId) -> String {
    format!("thumb_{}", id_file_name(key))
}

/// A number for the address of a picture that is the same in every run.
fn picture_id(thumb: &str) -> String {
    // FNV-1a.
    let hash = thumb.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// File name of the cached cover `thumb` of an item; sharper versions get
/// a suffix.
fn file_name(key: ItemId, thumb: &str, scale: u32) -> String {
    let name = format!("{}~{}", name_start(key), picture_id(thumb));
    match scale {
        1 => name,
        scale => format!("{name}@{scale}"),
    }
}

/// The key of the item a file in the cache is a cover of. Earlier versions
/// named covers `thumb_<key>` and `thumb_<key>@2`.
fn cover_key(file_name: &str) -> Option<ItemId> {
    let rest = file_name.strip_prefix("thumb_")?;
    let key = rest.split(['~', '@']).next()?;
    id_from_file_name(key)
}

/// The covers kept in `dir` for an item, newest first, whatever picture or
/// resolution they are.
fn covers_of(dir: &Path, key: ItemId) -> Vec<PathBuf> {
    let Ok(files) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, PathBuf)> = files
        .flatten()
        .filter(|file| {
            let name = file.file_name();
            let name = name.to_string_lossy();
            cover_key(&name) == Some(key) && !name.ends_with(".part")
        })
        .filter_map(|file| Some((file.metadata().ok()?.modified().ok()?, file.path())))
        .collect();
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, path)| path).collect()
}

/// A cover that was downloaded earlier, for when the server is out of reach.
pub async fn cached(machine_identifier: &str, key: ItemId) -> Option<gdk::Texture> {
    let dir = cache_dir_of(machine_identifier);
    let file = gio::spawn_blocking(move || {
        covers_of(&dir, key)
            .into_iter()
            .find_map(|path| gdk::Texture::from_filename(path).ok())
    })
    .await;
    file.ok().flatten()
}

/// A cached cover of the item, whatever its resolution; for other programs
/// such as the desktop's media controls.
pub fn cached_cover(server: &Server, key: ItemId) -> Option<PathBuf> {
    let dir = cache_dir(server);
    let shown = SHOWN.with_borrow(|shown| shown.get(&(dir.clone(), key)).cloned());
    shown
        .filter(|path| path.exists())
        .or_else(|| covers_of(&dir, key).into_iter().next())
}

/// Removes the covers of `server` that were not shown for a long time, in
/// the background. Those of items with an offline copy stay: they are all
/// there is to show without the server.
pub fn tidy(server: &Server) {
    let dir = cache_dir(server);
    // Not awaited: nothing depends on it.
    drop(gio::spawn_blocking(move || {
        let keep = crate::sync::cover_keys_in(&dir);
        remove_unused(&dir, &keep, UNUSED_AFTER);
    }));
}

fn remove_unused(dir: &Path, keep: &std::collections::HashSet<ItemId>, unused_after: Duration) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    for file in files.flatten() {
        let name = file.file_name();
        let Some(key) = name.to_str().and_then(cover_key) else {
            continue;
        };
        let unused = older_than(file.metadata().and_then(|file| file.modified()), unused_after);
        if unused && !keep.contains(&key) {
            if let Err(err) = std::fs::remove_file(file.path()) {
                log::warn!("Could not remove {}: {err}", file.path().display());
            }
        }
    }
}

/// The image `thumb` (an item's `thumb`, `composite`, …), cached under `key`.
/// Returns `None` when it cannot be loaded; the reason is logged.
pub async fn cover(server: &Server, key: ItemId, thumb: &str) -> Option<gdk::Texture> {
    let scale = scale();
    let dir = cache_dir(server);
    let path = dir.join(file_name(key, thumb, scale));
    SHOWN.with_borrow_mut(|shown| shown.insert((dir, key), path.clone()));
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
            let bytes = girens_model::net::download(&url)
                .await
                .map_err(|err| err.to_string())?;
            // Write next to the target and rename, so that a partial download
            // is never mistaken for a cached image.
            let mut partial = file.clone().into_os_string();
            partial.push(".part");
            let partial = PathBuf::from(partial);
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
        log::warn!("Could not download the image for {key}: {err}");
        return None;
    }

    let file = path.clone();
    let decoded = gio::spawn_blocking(move || {
        let texture = gdk::Texture::from_filename(&file);
        if texture.is_err() {
            // Do not keep a file that cannot be shown.
            let _ = std::fs::remove_file(&file);
        } else if older_than(std::fs::metadata(&file).and_then(|file| file.modified()), RENEW_AFTER) {
            // Its date tells when it was last of use; see `tidy`.
            let _ = std::fs::File::options()
                .write(true)
                .open(&file)
                .and_then(|file| file.set_modified(SystemTime::now()));
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
            log::warn!("Could not decode {}: {err}", path.display());
            None
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_picture_of_an_item_is_another_file() {
        let key: ItemId = 12.into();
        let first = file_name(key, "/library/metadata/12/thumb/1700000000", 1);
        let second = file_name(key, "/library/metadata/12/thumb/1700000999", 1);
        assert_ne!(first, second);
        // The same in every run.
        assert_eq!(first, "thumb_12~cfb0996a5d550ef5");
        assert_eq!(file_name(key, "/library/metadata/12/thumb/1700000000", 2), format!("{first}@2"));

        for name in [first.as_str(), second.as_str(), "thumb_12", "thumb_12@2"] {
            assert_eq!(cover_key(name), Some(key), "{name}");
        }
        assert_eq!(cover_key("thumb_123"), Some(123.into()));
        assert_eq!(cover_key("item_12"), None);
        // An id cannot take a cover out of the folder.
        assert!(!file_name("../12".into(), "t", 1).contains('/'));
    }

    #[test]
    fn covers_nobody_looked_at_are_removed() {
        let dir = std::env::temp_dir().join(format!("girens-images-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(1000);
        let cover = |name: &str, old: bool| {
            let path = dir.join(name);
            std::fs::write(&path, "picture").unwrap();
            if old {
                let file = std::fs::File::options().write(true).open(&path).unwrap();
                file.set_modified(long_ago).unwrap();
            }
            path
        };
        let old = cover(&file_name(1.into(), "a", 1), true);
        let legacy = cover("thumb_1@2", true);
        let renewed = cover(&file_name(1.into(), "b", 1), false);
        let offline = cover(&file_name(2.into(), "a", 1), true);
        let copy = cover("item_1", true);

        // The newest is what is shown without the server.
        assert_eq!(covers_of(&dir, 1.into())[0], renewed);
        assert_eq!(covers_of(&dir, 1.into()).len(), 3);

        let keep = std::collections::HashSet::from([ItemId::from(2)]);
        remove_unused(&dir, &keep, Duration::from_secs(500));
        assert!(!old.exists() && !legacy.exists());
        assert!(renewed.exists() && offline.exists() && copy.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
