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

/// The offline copy of an item, if there is one.
pub fn downloaded(server: &PlexServer, rating_key: u64) -> Option<PathBuf> {
    Some(local_path(server, rating_key)).filter(|path| path.exists())
}

impl Sync {
    pub fn current() -> Rc<Self> {
        CURRENT.with(Rc::clone)
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
                report!("Sync failed: {err}");
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

    /// Stops the running sync; the download in progress is discarded.
    pub fn cancel(&self) {
        let Some(task) = self.task.take() else {
            return;
        };
        // Dropping the task also aborts the network request it waits for.
        task.abort();
        if let Some(server) = Session::current().server() {
            remove_partial_downloads(&server);
        }
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
                if downloaded(server, item.rating_key).is_none() {
                    self.emit(Event::Downloading(Box::new(item.clone()), true));
                    let result = self.download(server, &item, entry.converted).await;
                    self.emit(Event::Downloading(Box::new(item.clone()), false));
                    if let Err(err) = result {
                        report!("Could not download {}: {err}", item.rating_key);
                    }
                }
            }
        }

        // Remove what an interrupted download left behind, and copies of
        // items that are no longer synced.
        remove_partial_downloads(server);
        if let Ok(files) = std::fs::read_dir(images::cache_dir(server)) {
            for file in files.flatten() {
                let name = file.file_name();
                let Some(key) = name.to_str().and_then(|name| name.strip_prefix("item_")) else {
                    continue;
                };
                if key.parse().is_ok_and(|key: u64| !wanted.contains(&key)) {
                    if let Err(err) = std::fs::remove_file(file.path()) {
                        eprintln!("Could not remove {}: {err}", file.path().display());
                    }
                }
            }
        }
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
    ) -> Result<(), girens_plex::Error> {
        let key = item.rating_key;
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
                let result =
                    girens_plex::download_to_file_with_progress(url.as_str(), &target, progress)
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
                girens_plex::download_to_file_with_progress(url.as_str(), &target, progress).await
            }
        })
        .await
    }
}

/// Deletes `item_*.tmp` files, the remains of downloads that did not finish.
fn remove_partial_downloads(server: &PlexServer) {
    let Ok(files) = std::fs::read_dir(images::cache_dir(server)) else {
        return;
    };
    for file in files.flatten() {
        let name = file.file_name();
        let partial = name
            .to_str()
            .is_some_and(|name| name.starts_with("item_") && name.ends_with(".tmp"));
        if partial {
            let _ = std::fs::remove_file(file.path());
        }
    }
}
