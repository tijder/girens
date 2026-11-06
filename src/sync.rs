//! Offline copies: the list of items to keep on this device and the job
//! that downloads them.
//!
//! The files are shared by everyone who uses a server on this device
//! (`<cache>/<server>/item_<key>`, with the details of the item next to it in
//! `item_<key>.json`). What each user wants is kept per user: the list of
//! synced items in [`store`], and which copies that comes down to, with how
//! far the user has watched them, in `<cache>/<server>/user_<user>.json`. A
//! user sees only the own copies, and a file goes when nobody wants it.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use girens_plex::model::{Item, ItemKind};
use girens_plex::playback::TranscodeRequest;
use girens_plex::PlexServer;
use gtk::prelude::*;
use gtk::{gio, glib};
use serde::{Deserialize, Serialize};

use crate::session::Session;
use crate::store::{self, SyncEntry};
use crate::{config, images, runtime};

#[derive(Debug, Clone)]
pub enum Event {
    /// A sync run started or finished.
    Busy(bool),
    /// A download started (`true`) or ended.
    Downloading(Box<Item>, bool),
    /// How far the download of the item with this rating key is, from 0 to
    /// 1; `None` when the size is unknown (a transcoded copy).
    Progress(u64, Option<f64>),
    /// The list of synced items changed.
    EntriesChanged,
}

#[derive(Default)]
pub struct Sync {
    busy: Cell<bool>,
    /// Rating keys of the items with an offline copy, so that tiles can
    /// tell without touching the disk.
    downloaded: RefCell<HashSet<u64>>,
    task: RefCell<Option<glib::JoinHandle<()>>>,
    listeners: RefCell<Vec<Rc<dyn Fn(&Event)>>>,
}

thread_local! {
    static CURRENT: Rc<Sync> = Rc::default();
}

/// Where the offline copy of an item is, or would be, stored.
pub fn local_path(server: &PlexServer, rating_key: u64) -> PathBuf {
    images::cache_dir(server).join(format!("item_{rating_key}"))
}

/// The offline copy `user` has of an item of the server with this
/// identifier, if any. Works without a connection.
pub fn local_file(machine_identifier: &str, user: &str, rating_key: u64) -> Option<PathBuf> {
    let dir = images::cache_dir_of(machine_identifier);
    if !Manifest::read(&dir, user).items.contains_key(&rating_key) {
        return None;
    }
    Some(dir.join(format!("item_{rating_key}"))).filter(|path| path.exists())
}

/// The description of an offline copy, stored next to it so that it can be
/// shown and played without the server.
fn sidecar(dir: &Path, rating_key: u64) -> PathBuf {
    dir.join(format!("item_{rating_key}.json"))
}

fn write_sidecar(dir: &Path, item: &Item) {
    match serde_json::to_string(item) {
        Ok(text) => {
            if let Err(err) = std::fs::write(sidecar(dir, item.rating_key), text) {
                eprintln!("Could not store the details of {}: {err}", item.rating_key);
            }
        }
        Err(err) => eprintln!("Could not store the details of {}: {err}", item.rating_key),
    }
}

/// How far a user is with an item: as the server told when it was synced,
/// or as watched from the offline copy since.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Watched {
    view_offset: u64,
    view_count: u32,
    user_rating: Option<f32>,
    /// Watched without the server knowing; it is told when it is back.
    unsent: bool,
}

/// From where an item counts as watched to the end.
const WATCHED_FRACTION: f64 = 0.9;

impl Watched {
    fn of(item: &Item) -> Self {
        Self {
            view_offset: item.view_offset,
            view_count: item.view_count,
            user_rating: item.user_rating,
            unsent: false,
        }
    }

    /// What to keep for `item` after asking the server: what it says,
    /// unless `self` holds progress it has not heard of yet.
    fn updated(self, item: &Item) -> Self {
        if self.unsent {
            self
        } else {
            Self::of(item)
        }
    }
}

/// The offline copies of one user on one server.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Manifest {
    items: BTreeMap<u64, Watched>,
}

const MANIFEST_PREFIX: &str = "user_";

impl Manifest {
    fn path(dir: &Path, user: &str) -> PathBuf {
        // A user key may hold characters that do not belong in a file name.
        let name: String = user
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
            .collect();
        dir.join(format!("{MANIFEST_PREFIX}{name}.json"))
    }

    fn read_file(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn read(dir: &Path, user: &str) -> Self {
        Self::read_file(&Self::path(dir, user))
    }

    fn write(&self, dir: &Path, user: &str) {
        let path = Self::path(dir, user);
        let written = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&path, serde_json::to_string(self).unwrap_or_default()));
        if let Err(err) = written {
            eprintln!("Could not write {}: {err}", path.display());
        }
    }

    /// The manifests of everyone with copies in `dir`.
    fn files(dir: &Path) -> Vec<PathBuf> {
        let Ok(files) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        files
            .flatten()
            .map(|file| file.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(MANIFEST_PREFIX) && name.ends_with(".json")
                    })
            })
            .collect()
    }

    /// The items anyone has a copy of in `dir`.
    fn wanted_by_anyone(dir: &Path) -> HashSet<u64> {
        Self::files(dir)
            .iter()
            .flat_map(|path| Self::read_file(path).items.into_keys())
            .collect()
    }
}

/// The rating key in the name of a copy, complete or partial, or of its
/// description.
fn copy_key(file_name: &str) -> Option<u64> {
    let key = file_name.strip_prefix("item_")?;
    key.strip_suffix(".tmp")
        .or_else(|| key.strip_suffix(".json"))
        .unwrap_or(key)
        .parse()
        .ok()
}

/// Removes the copies in `dir`, complete or partial, of items not in `keep`.
fn prune(dir: &Path, keep: &HashSet<u64>) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    for file in files.flatten() {
        let name = file.file_name();
        let unwanted = name
            .to_str()
            .and_then(copy_key)
            .is_some_and(|key| !keep.contains(&key));
        if unwanted {
            if let Err(err) = std::fs::remove_file(file.path()) {
                eprintln!("Could not remove {}: {err}", file.path().display());
            }
        }
    }
}

/// The complete copies in `dir`, whoever they are of.
fn copies(dir: &Path) -> HashSet<u64> {
    std::fs::read_dir(dir)
        .map(|files| {
            files
                .flatten()
                .filter_map(|file| {
                    file.file_name()
                        .to_str()?
                        .strip_prefix("item_")?
                        .parse()
                        .ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Makes the copies that were made before they were kept per user those of
/// `user`. Does nothing once anyone has copies of their own.
pub fn adopt_downloads(machine_identifier: &str, user: &str) {
    adopt_downloads_in(&images::cache_dir_of(machine_identifier), user);
}

fn adopt_downloads_in(dir: &Path, user: &str) {
    if !Manifest::files(dir).is_empty() {
        return;
    }
    let items: BTreeMap<u64, Watched> = copies(dir)
        .into_iter()
        .map(|key| {
            let watched = std::fs::read_to_string(sidecar(dir, key))
                .ok()
                .and_then(|text| serde_json::from_str::<Item>(&text).ok())
                .map(|item| Watched::of(&item))
                .unwrap_or_default();
            (key, watched)
        })
        .collect();
    if !items.is_empty() {
        Manifest { items }.write(dir, user);
    }
}

/// Forgets the offline copies of users that are gone, on every server, and
/// removes the files nobody else wants.
pub fn forget_users(users: &[String]) {
    if users.is_empty() {
        return;
    }
    let Ok(servers) = std::fs::read_dir(glib::user_cache_dir()) else {
        return;
    };
    for server in servers.flatten() {
        forget_users_in(&server.path(), users);
    }
}

fn forget_users_in(dir: &Path, users: &[String]) {
    let mut removed = false;
    for user in users {
        removed |= std::fs::remove_file(Manifest::path(dir, user)).is_ok();
    }
    if removed {
        prune(dir, &Manifest::wanted_by_anyone(dir));
    }
}

/// The items `user` has an offline copy of for this server, as far as their
/// details are known.
pub fn offline_items(machine_identifier: &str, user: &str) -> Vec<Item> {
    offline_items_in(&images::cache_dir_of(machine_identifier), user)
}

fn offline_items_in(dir: &Path, user: &str) -> Vec<Item> {
    let present = copies(dir);
    let mut items: Vec<Item> = Manifest::read(dir, user)
        .items
        .into_iter()
        .filter(|(key, _)| present.contains(key))
        .filter_map(|(key, watched)| {
            let text = std::fs::read_to_string(sidecar(dir, key)).ok()?;
            let mut item: Item = serde_json::from_str(&text).ok()?;
            // The details are shared; how far someone is, is not.
            item.view_offset = watched.view_offset;
            item.view_count = watched.view_count;
            item.user_rating = watched.user_rating;
            Some(item)
        })
        .collect();
    items.sort_by(|a, b| {
        (&a.grandparent_title, &a.parent_index, &a.index, &a.title).cmp(&(
            &b.grandparent_title,
            &b.parent_index,
            &b.index,
            &b.title,
        ))
    });
    items
}

/// Remembers how far `user` is in an offline copy, so that it resumes there
/// without the server too. `duration` tells when it is watched to the end.
/// The server is still to be told; see [`progress_sent`].
pub fn record_progress(
    machine_identifier: &str,
    user: &str,
    rating_key: u64,
    position: u64,
    duration: Option<u64>,
) {
    let dir = images::cache_dir_of(machine_identifier);
    record_progress_in(&dir, user, rating_key, position, duration);
}

fn record_progress_in(
    dir: &Path,
    user: &str,
    rating_key: u64,
    position: u64,
    duration: Option<u64>,
) {
    let mut manifest = Manifest::read(dir, user);
    let Some(watched) = manifest.items.get_mut(&rating_key) else {
        return;
    };
    let finished = duration
        .is_some_and(|duration| duration > 0 && position as f64 >= duration as f64 * WATCHED_FRACTION);
    if finished {
        watched.view_offset = 0;
        watched.view_count = watched.view_count.max(1);
    } else {
        watched.view_offset = position;
    }
    watched.unsent = true;
    manifest.write(dir, user);
}

/// Notes that the server knows how far `user` is in an offline copy.
pub fn progress_sent(machine_identifier: &str, user: &str, rating_key: u64) {
    progress_sent_in(&images::cache_dir_of(machine_identifier), user, rating_key);
}

fn progress_sent_in(dir: &Path, user: &str, rating_key: u64) {
    let mut manifest = Manifest::read(dir, user);
    if let Some(watched) = manifest.items.get_mut(&rating_key) {
        if std::mem::take(&mut watched.unsent) {
            manifest.write(dir, user);
        }
    }
}

/// The offline copies `user` watched without the server, with how far.
fn unsent_progress(dir: &Path, user: &str) -> Vec<(Item, Watched)> {
    Manifest::read(dir, user)
        .items
        .into_iter()
        .filter(|(_, watched)| watched.unsent)
        .filter_map(|(key, watched)| {
            let text = std::fs::read_to_string(sidecar(dir, key)).ok()?;
            Some((serde_json::from_str(&text).ok()?, watched))
        })
        .collect()
}

/// Tells the server what `user` watched from offline copies while it was
/// out of reach.
pub async fn push_progress(server: &PlexServer, user: &str) {
    let dir = images::cache_dir(server);
    for (item, watched) in unsent_progress(&dir, user) {
        let key = item.rating_key;
        let sent = {
            let server = server.clone();
            runtime::run(async move {
                if watched.view_offset > 0 {
                    let stopped = girens_plex::playback::PlaybackState::Stopped;
                    server
                        .update_timeline(&item, watched.view_offset, stopped, None)
                        .await
                } else {
                    server.mark_watched(key).await
                }
            })
            .await
        };
        match sent {
            Ok(()) => progress_sent_in(&dir, user, key),
            Err(err) => {
                eprintln!("Could not tell the server how far item {key} was watched: {err}");
                // The others will not get through either.
                if err.is_network() {
                    return;
                }
            }
        }
    }
}

/// The offline copy of an item, if there is one, whoever asked for it.
fn downloaded(server: &PlexServer, rating_key: u64) -> Option<PathBuf> {
    Some(local_path(server, rating_key)).filter(|path| path.exists())
}

/// The active user and the server whose copies are meant.
fn scope() -> Option<(String, String)> {
    let session = Session::current();
    Some((session.user_key()?, session.machine_identifier()?))
}

impl Sync {
    pub fn current() -> Rc<Self> {
        CURRENT.with(Rc::clone)
    }

    /// Whether the active user has an offline copy of the item.
    pub fn is_downloaded(&self, rating_key: u64) -> bool {
        self.downloaded.borrow().contains(&rating_key)
    }

    /// Looks on the disk which items `user` has an offline copy of.
    pub fn rescan(&self, machine_identifier: &str, user: &str) {
        let dir = images::cache_dir_of(machine_identifier);
        let present = copies(&dir);
        let keys = Manifest::read(&dir, user)
            .items
            .into_keys()
            .filter(|key| present.contains(key))
            .collect();
        self.downloaded.replace(keys);
    }

    /// The items the active user wants to have offline of the server in
    /// use.
    pub fn entries(&self) -> Vec<SyncEntry> {
        scope()
            .map(|(user, machine)| store::sync_entries(&user, &machine))
            .unwrap_or_default()
    }

    /// Forgets whose copies were looked up, when someone else takes over.
    pub fn forget(&self) {
        self.downloaded.borrow_mut().clear();
        self.emit(Event::EntriesChanged);
    }

    pub fn connect(&self, listener: impl Fn(&Event) + 'static) {
        self.listeners.borrow_mut().push(Rc::new(listener));
    }

    fn emit(&self, event: Event) {
        let listeners: Vec<_> = self.listeners.borrow().clone();
        for listener in listeners {
            listener(&event);
        }
    }

    pub fn add(self: &Rc<Self>, entry: SyncEntry) {
        let Some((user, machine)) = scope() else {
            return;
        };
        store::add_sync_entry(&user, &machine, &entry);
        self.emit(Event::EntriesChanged);
        self.run();
    }

    pub fn remove(&self, rating_key: u64) {
        let Some((user, machine)) = scope() else {
            return;
        };
        store::remove_sync_entry(&user, &machine, rating_key);
        self.emit(Event::EntriesChanged);
    }

    /// Downloads what is missing and deletes copies that are no longer
    /// wanted. Does nothing while a run is in progress.
    pub fn run(self: &Rc<Self>) {
        let session = Session::current();
        let (Some(server), Some(user)) = (session.server(), session.user_key()) else {
            return;
        };
        if self.busy.replace(true) {
            return;
        }
        self.emit(Event::Busy(true));
        let sync = self.clone();
        let task = glib::spawn_future_local(async move {
            // Before the server is asked how far the user is.
            push_progress(&server, &user).await;
            if let Err(err) = sync.run_once(&server, &user).await {
                crate::notify::failure(&crate::i18n::gettext("Sync failed"), &err);
            }
            sync.task.take();
            sync.busy.set(false);
            sync.emit(Event::Busy(false));
        });
        self.task.replace(Some(task));
    }

    pub fn is_busy(&self) -> bool {
        self.busy.get()
    }

    /// Stops the running sync. The download in progress is kept and continued
    /// by the next run.
    pub fn cancel(&self) {
        let Some(task) = self.task.take() else {
            return;
        };
        // Dropping the task also aborts the network request it waits for.
        // What was downloaded so far stays, to be continued next time.
        task.abort();
        self.busy.set(false);
        self.emit(Event::Busy(false));
    }

    async fn run_once(&self, server: &PlexServer, user: &str) -> Result<(), girens_plex::Error> {
        let dir = images::cache_dir(server);
        let mut wanted = HashSet::new();
        // What the user has a copy of when this run is over.
        let mut manifest = Manifest::default();
        let before = Manifest::read(&dir, user);
        let watched = |item: &Item| {
            before
                .items
                .get(&item.rating_key)
                .map_or_else(|| Watched::of(item), |known| known.updated(item))
        };
        for entry in store::sync_entries(user, server.machine_identifier()) {
            let items = {
                let server = server.clone();
                runtime::run(async move { expand(&server, entry.rating_key).await }).await
            };
            let items = match items {
                Ok(items) => items,
                Err(girens_plex::Error::NotFound(key)) => {
                    eprintln!("Synced item {key} no longer exists on the server");
                    continue;
                }
                // Without the full list nothing may be deleted below.
                Err(err) => return Err(err),
            };
            let selected = items
                .into_iter()
                .filter(|item| {
                    let video = matches!(item.kind, ItemKind::Movie | ItemKind::Episode);
                    !entry.only_unwatched || (video && !item.is_watched())
                })
                .take(entry.max_items.map_or(usize::MAX, |max| max as usize));
            for item in selected {
                wanted.insert(item.rating_key);
                if downloaded(server, item.rating_key).is_some() {
                    // Copies from before the details were stored get them
                    // now. The file may be there for someone else; it
                    // serves this user as well.
                    if !sidecar(&dir, item.rating_key).exists() {
                        write_sidecar(&dir, &item);
                    }
                    manifest.items.insert(item.rating_key, watched(&item));
                    self.downloaded.borrow_mut().insert(item.rating_key);
                } else {
                    self.emit(Event::Downloading(Box::new(item.clone()), true));
                    let result = self.download(server, &item, entry.converted).await;
                    if result.is_ok() {
                        write_sidecar(&dir, &item);
                        manifest.items.insert(item.rating_key, watched(&item));
                        self.downloaded.borrow_mut().insert(item.rating_key);
                        // So that the copy is the user's even when the
                        // rest of the run is cancelled.
                        let mut so_far = Manifest::read(&dir, user);
                        so_far.items.insert(item.rating_key, Watched::of(&item));
                        so_far.write(&dir, user);
                    }
                    self.emit(Event::Downloading(Box::new(item.clone()), false));
                    if let Err(err) = result {
                        crate::notify::failure(
                            &crate::i18n::gettext("Could not download an item"),
                            &err,
                        );
                    }
                }
            }
        }

        manifest.write(&dir, user);
        // Remove copies, complete or partial, of items that nobody syncs
        // any more. A partial copy of an item this user wants stays: the
        // next run continues it.
        let mut keep = Manifest::wanted_by_anyone(&dir);
        keep.extend(wanted);
        prune(&dir, &keep);
        self.rescan(server.machine_identifier(), user);
        Ok(())
    }
}

/// The playable items behind a synced item.
async fn expand(server: &PlexServer, rating_key: u64) -> Result<Vec<Item>, girens_plex::Error> {
    let item = server.item(rating_key).await?;
    match item.kind {
        ItemKind::Movie | ItemKind::Episode => Ok(vec![item]),
        ItemKind::Album => server.children(rating_key).await,
        ItemKind::Artist | ItemKind::Show => server.all_leaves(rating_key).await,
        ItemKind::Playlist => server.playlist_items(rating_key).await,
        _ => Ok(Vec::new()),
    }
}

impl Sync {
    /// Downloads one item, reporting progress twice a second.
    async fn download(
        &self,
        server: &PlexServer,
        item: &Item,
        converted: bool,
    ) -> Result<(), String> {
        let key = item.rating_key;
        // An original file has a known size: refuse it when it does not fit,
        // rather than filling the disk.
        if let (false, Some(size)) = (converted, item.parts().next().and_then(|part| part.size)) {
            let free = free_space(&images::cache_dir(server));
            if free.is_some_and(|free| free < size.saturating_add(DISK_RESERVE)) {
                return Err(crate::i18n::gettext(
                    "Not enough disk space for the download",
                ));
            }
        }
        let counters = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
        let reporter = glib::timeout_add_local(Duration::from_millis(500), {
            let counters = counters.clone();
            move || {
                let received = counters.0.load(Ordering::Relaxed);
                let total = counters.1.load(Ordering::Relaxed);
                let fraction = (total > 0).then(|| received as f64 / total as f64);
                Sync::current().emit(Event::Progress(key, fraction));
                glib::ControlFlow::Continue
            }
        });
        // Stops the reports when the download ends or is cancelled.
        struct StopOnDrop(Option<glib::SourceId>);
        impl Drop for StopOnDrop {
            fn drop(&mut self) {
                if let Some(source) = self.0.take() {
                    source.remove();
                }
            }
        }
        let _reporter = StopOnDrop(Some(reporter));

        let target = local_path(server, key);
        let (server, item) = (server.clone(), item.clone());
        let resolution = gio::Settings::new(config::APP_ID)
            .string("transcode-media-to-resolution")
            .to_string();
        let session = glib::uuid_string_random().to_string();
        runtime::run(async move {
            let progress = move |received: u64, total: Option<u64>| {
                counters.0.store(received, Ordering::Relaxed);
                counters.1.store(total.unwrap_or(0), Ordering::Relaxed);
            };
            if converted && !item.kind.is_audio() {
                let request = TranscodeRequest {
                    session,
                    video_resolution: Some(resolution),
                    offset: 0,
                };
                let url = server.transcode_download_url(&item, &request);
                // A transcode cannot be continued, so it always starts over.
                let result = girens_plex::download_to_file_with_progress(
                    url.as_str(),
                    &target,
                    false,
                    progress,
                )
                .await;
                if let Err(err) = server.stop_transcode_session(&item, &request.session).await {
                    eprintln!("Could not end the transcode session: {err}");
                }
                result
            } else {
                let part = item
                    .parts()
                    .next()
                    .ok_or(girens_plex::Error::NotFound(item.rating_key))?;
                let url = server.download_url(&part.key);
                girens_plex::download_to_file_with_progress(url.as_str(), &target, true, progress)
                    .await
            }
        })
        .await
        .map_err(|err: girens_plex::Error| err.to_string())
    }
}

/// Space to leave free on the disk after a download.
const DISK_RESERVE: u64 = 500 * 1024 * 1024;

/// Free bytes on the file system that holds `dir`.
fn free_space(dir: &std::path::Path) -> Option<u64> {
    // The directory may not exist yet; its parent is on the same disk.
    let existing = dir.ancestors().find(|path| path.exists())?;
    let info = gio::File::for_path(existing)
        .query_filesystem_info(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE, gio::Cancellable::NONE)
        .ok()?;
    Some(info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_dir(test: impl FnOnce(&Path)) {
        let dir = std::env::temp_dir().join(format!(
            "girens-sync-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        test(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A copy with its details, as a sync run leaves it.
    fn copy(dir: &Path, rating_key: u64, view_offset: u64) {
        std::fs::write(dir.join(format!("item_{rating_key}")), "media").unwrap();
        let details = serde_json::json!({
            "ratingKey": rating_key.to_string(),
            "key": format!("/library/metadata/{rating_key}"),
            "type": "movie",
            "title": format!("Movie {rating_key}"),
            "viewOffset": view_offset,
        });
        std::fs::write(sidecar(dir, rating_key), details.to_string()).unwrap();
    }

    fn manifest(dir: &Path, user: &str, keys: &[u64]) {
        Manifest {
            items: keys
                .iter()
                .map(|key| (*key, Watched::default()))
                .collect(),
        }
        .write(dir, user);
    }

    fn titles(dir: &Path, user: &str) -> Vec<String> {
        offline_items_in(dir, user)
            .into_iter()
            .map(|item| item.title)
            .collect()
    }

    #[test]
    fn copies_from_before_become_those_of_the_user_who_was_signed_in() {
        with_dir(|dir| {
            copy(dir, 1, 60_000);
            copy(dir, 2, 0);
            std::fs::write(dir.join("item_3.tmp"), "half").unwrap();
            std::fs::write(dir.join("thumb_1"), "cover").unwrap();

            adopt_downloads_in(dir, "me");
            assert_eq!(titles(dir, "me"), ["Movie 1", "Movie 2"]);
            assert_eq!(offline_items_in(dir, "me")[0].view_offset, 60_000);
            assert!(titles(dir, "child").is_empty());

            // Not again once someone has copies of their own.
            copy(dir, 4, 0);
            adopt_downloads_in(dir, "child");
            assert!(titles(dir, "child").is_empty());
            assert_eq!(titles(dir, "me"), ["Movie 1", "Movie 2"]);
        });
    }

    #[test]
    fn everyone_sees_the_own_copies_with_the_own_progress() {
        with_dir(|dir| {
            copy(dir, 1, 60_000);
            copy(dir, 2, 0);
            manifest(dir, "me", &[1, 2]);
            let mut child = Manifest::default();
            child.items.insert(
                2,
                Watched {
                    view_offset: 5_000,
                    view_count: 1,
                    ..Watched::default()
                },
            );
            // Wanted, but the file is not there (yet).
            child.items.insert(9, Watched::default());
            child.write(dir, "child");

            assert_eq!(titles(dir, "me"), ["Movie 1", "Movie 2"]);
            // The progress in the shared details is someone else's.
            assert_eq!(offline_items_in(dir, "me")[0].view_offset, 0);
            let items = offline_items_in(dir, "child");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].title, "Movie 2");
            assert_eq!(items[0].view_offset, 5_000);
            assert_eq!(items[0].view_count, 1);
        });
    }

    #[test]
    fn what_is_watched_offline_is_kept_until_the_server_knows() {
        with_dir(|dir| {
            copy(dir, 1, 0);
            copy(dir, 2, 0);
            manifest(dir, "me", &[1, 2]);
            manifest(dir, "child", &[1]);

            // Half way through the first, to the end of the second.
            record_progress_in(dir, "me", 1, 30 * 60_000, Some(60 * 60_000));
            record_progress_in(dir, "me", 2, 56 * 60_000, Some(60 * 60_000));
            // Not a copy of this user: nothing to remember.
            record_progress_in(dir, "me", 3, 1_000, None);

            let items = offline_items_in(dir, "me");
            assert_eq!(items[0].view_offset, 30 * 60_000);
            assert_eq!(items[0].view_count, 0);
            assert_eq!((items[1].view_offset, items[1].view_count), (0, 1));
            assert_eq!(offline_items_in(dir, "child")[0].view_offset, 0);

            let unsent: Vec<u64> = unsent_progress(dir, "me")
                .iter()
                .map(|(item, _)| item.rating_key)
                .collect();
            assert_eq!(unsent, [1, 2]);
            assert!(unsent_progress(dir, "child").is_empty());

            // A sync that hears older news from the server keeps this.
            let stale: Item =
                serde_json::from_str(&std::fs::read_to_string(sidecar(dir, 1)).unwrap()).unwrap();
            let known = Manifest::read(dir, "me").items[&1];
            assert_eq!(known.updated(&stale).view_offset, 30 * 60_000);

            progress_sent_in(dir, "me", 1);
            let unsent = unsent_progress(dir, "me");
            assert_eq!(unsent.len(), 1);
            assert_eq!(unsent[0].0.rating_key, 2);
            // The position stays for resuming; the server has it now too.
            let known = Manifest::read(dir, "me").items[&1];
            assert_eq!(known.view_offset, 30 * 60_000);
            assert_eq!(known.updated(&stale).view_offset, 0);
        });
    }

    #[test]
    fn a_file_stays_for_as_long_as_someone_wants_it() {
        with_dir(|dir| {
            for key in [1, 2, 3] {
                copy(dir, key, 0);
            }
            std::fs::write(dir.join("item_3.tmp"), "half").unwrap();
            std::fs::write(dir.join("thumb_1"), "cover").unwrap();
            manifest(dir, "me", &[1, 2]);
            manifest(dir, "child", &[2, 3]);
            manifest(dir, "server:abc", &[1]);
            assert!(Manifest::path(dir, "server:abc")
                .to_string_lossy()
                .ends_with("user_server_abc.json"));

            forget_users_in(dir, &["me".to_owned()]);
            assert!(titles(dir, "me").is_empty());
            assert_eq!(copies(dir), HashSet::from([1, 2, 3]));

            forget_users_in(dir, &["child".to_owned(), "nobody".to_owned()]);
            assert_eq!(copies(dir), HashSet::from([1]));
            assert!(!dir.join("item_3.tmp").exists());
            assert!(!sidecar(dir, 2).exists());
            // Covers are not copies.
            assert!(dir.join("thumb_1").exists());
            assert_eq!(titles(dir, "server:abc"), ["Movie 1"]);
        });
    }
}
