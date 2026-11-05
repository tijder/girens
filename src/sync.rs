//! Offline copies: the list of items to keep on this device and the job
//! that downloads them. The list lives in the config file of the Python
//! version and the files keep their names (`<cache>/<server>/item_<key>`).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use girens_plex::model::{Item, ItemKind};
use girens_plex::playback::TranscodeRequest;
use girens_plex::PlexServer;
use gtk::prelude::*;
use gtk::{gio, glib};

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

/// The offline copy of an item of the server with this identifier, if any.
/// Works without a connection.
pub fn local_file(machine_identifier: &str, rating_key: u64) -> Option<PathBuf> {
    Some(images::cache_dir_of(machine_identifier).join(format!("item_{rating_key}")))
        .filter(|path| path.exists())
}

/// The description of an offline copy, stored next to it so that it can be
/// shown and played without the server.
fn sidecar(dir: &std::path::Path, rating_key: u64) -> PathBuf {
    dir.join(format!("item_{rating_key}.json"))
}

fn write_sidecar(dir: &std::path::Path, item: &Item) {
    match serde_json::to_string(item) {
        Ok(text) => {
            if let Err(err) = std::fs::write(sidecar(dir, item.rating_key), text) {
                eprintln!("Could not store the details of {}: {err}", item.rating_key);
            }
        }
        Err(err) => eprintln!("Could not store the details of {}: {err}", item.rating_key),
    }
}

/// The items with an offline copy for this server, as far as their details
/// are known.
pub fn offline_items(machine_identifier: &str) -> Vec<Item> {
    let dir = images::cache_dir_of(machine_identifier);
    let Ok(files) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut items: Vec<Item> = files
        .flatten()
        .filter_map(|file| {
            let name = file.file_name();
            let key: u64 = name.to_str()?.strip_prefix("item_")?.parse().ok()?;
            let text = std::fs::read_to_string(sidecar(&dir, key)).ok()?;
            serde_json::from_str(&text).ok()
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

/// The offline copy of an item, if there is one.
pub fn downloaded(server: &PlexServer, rating_key: u64) -> Option<PathBuf> {
    Some(local_path(server, rating_key)).filter(|path| path.exists())
}

impl Sync {
    pub fn current() -> Rc<Self> {
        CURRENT.with(Rc::clone)
    }

    /// Whether the item has an offline copy.
    pub fn is_downloaded(&self, rating_key: u64) -> bool {
        self.downloaded.borrow().contains(&rating_key)
    }

    /// Looks on the disk which items have an offline copy.
    pub fn rescan(&self, machine_identifier: &str) {
        let keys = std::fs::read_dir(images::cache_dir_of(machine_identifier))
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
            .unwrap_or_default();
        self.downloaded.replace(keys);
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
        store::add_sync_entry(&entry);
        self.emit(Event::EntriesChanged);
        self.run();
    }

    pub fn remove(&self, rating_key: u64) {
        store::remove_sync_entry(rating_key);
        self.emit(Event::EntriesChanged);
    }

    /// Downloads what is missing and deletes copies that are no longer
    /// wanted. Does nothing while a run is in progress.
    pub fn run(self: &Rc<Self>) {
        let Some(server) = Session::current().server() else {
            return;
        };
        if self.busy.replace(true) {
            return;
        }
        self.emit(Event::Busy(true));
        let sync = self.clone();
        let task = glib::spawn_future_local(async move {
            if let Err(err) = sync.run_once(&server).await {
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

    async fn run_once(&self, server: &PlexServer) -> Result<(), girens_plex::Error> {
        let mut wanted = HashSet::new();
        for entry in store::sync_entries() {
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
                let dir = images::cache_dir(server);
                if downloaded(server, item.rating_key).is_some() {
                    // Copies from before the details were stored get them now.
                    if !sidecar(&dir, item.rating_key).exists() {
                        write_sidecar(&dir, &item);
                    }
                } else {
                    self.emit(Event::Downloading(Box::new(item.clone()), true));
                    let result = self.download(server, &item, entry.converted).await;
                    if result.is_ok() {
                        write_sidecar(&dir, &item);
                        self.downloaded.borrow_mut().insert(item.rating_key);
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

        // Remove copies, complete or partial, of items that are no longer
        // synced. A partial copy of a wanted item stays: the next run
        // continues it.
        if let Ok(files) = std::fs::read_dir(images::cache_dir(server)) {
            for file in files.flatten() {
                let name = file.file_name();
                let Some(key) = name.to_str().and_then(|name| name.strip_prefix("item_")) else {
                    continue;
                };
                let key = key
                    .strip_suffix(".tmp")
                    .or_else(|| key.strip_suffix(".json"))
                    .unwrap_or(key);
                if key.parse().is_ok_and(|key: u64| !wanted.contains(&key)) {
                    if let Err(err) = std::fs::remove_file(file.path()) {
                        eprintln!("Could not remove {}: {err}", file.path().display());
                    }
                }
            }
        }
        self.rescan(server.machine_identifier());
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
