//! Media playback with mpv.
//!
//! Everything runs on the main thread: mpv wakes
//! the main loop when it has events, and what happens when a file ends is
//! decided by an explicit [`EndAction`] instead of a set of flags.

pub mod render;

use girens_model::ItemId;
use std::cell::{OnceCell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use girens_model::model::{Item, ItemKind, LibrarySection, StreamKind};
use girens_model::{PlayQueue, PlaybackState};
use gtk::prelude::*;
use gtk::{gio, glib};
use libmpv2::events::{Event as MpvEvent, PropertyData};
use libmpv2::{Format, Mpv};

use crate::backend::{Opened, StreamOptions};
use crate::session::Session;
use crate::{config, runtime};

/// Seconds between position updates sent to the UI and to the server.
const UI_INTERVAL: f64 = 0.5;
const TIMELINE_INTERVAL: f64 = 5.0;
const PING_INTERVAL: u32 = 30;

/// Where playback of an item starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StartMode {
    /// Ask the user when the item was partly watched.
    Ask,
    /// Where the user left off; the beginning for something new.
    Resume,
    Beginning,
    /// At this position in milliseconds.
    At(u64),
}

/// What to play.
#[derive(Debug, Clone)]
pub enum PlayTarget {
    /// Any item, by rating key.
    Item(ItemId),
    /// A playlist, starting at one of its items; both by rating key.
    PlaylistItem { playlist: ItemId, item: ItemId },
    /// A whole library in the given sort order.
    Section {
        section: LibrarySection,
        sort: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    /// The item was partly watched; answer with [`Player::start_current`].
    AskResume(Box<Item>),
    /// Playback of an item began. `video` tells whether there is a picture.
    Started {
        item: Box<Item>,
        video: bool,
    },
    Paused(bool),
    /// Position in milliseconds.
    Position(u64),
    /// The play queue or the position in it changed.
    QueueChanged,
    /// Whether the item that is playing starts over when it ends.
    RepeatChanged(bool),
    /// Nothing is playing any more.
    Stopped,
}

/// What to do when mpv reports that the current file ended.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EndAction {
    /// It ended by itself: continue with the next item of the queue.
    Advance,
    Stop,
    Previous,
    Index(usize),
    /// Play the same item again from this position (milliseconds), e.g. after
    /// choosing another audio track of a transcoded stream.
    Restart(u64),
    /// Something else was requested while this was still playing.
    Replace(StartMode),
}

/// Which queue entry to start, and how, after the entry at `index` of a
/// queue of `length` ended with `action`. `None` means playback is over.
fn next_after(
    action: EndAction,
    index: usize,
    length: usize,
    failed: bool,
) -> Option<(usize, StartMode)> {
    match action {
        // Do not run through the whole queue when files fail to open.
        EndAction::Advance if failed => None,
        EndAction::Advance => (index + 1 < length).then_some((index + 1, StartMode::Ask)),
        EndAction::Previous => index.checked_sub(1).map(|index| (index, StartMode::Ask)),
        EndAction::Index(index) => (index < length).then_some((index, StartMode::Ask)),
        EndAction::Restart(position) => Some((index, StartMode::At(position))),
        EndAction::Replace(mode) => (index < length).then_some((index, mode)),
        EndAction::Stop => None,
    }
}

struct Playing {
    /// The entry of the play queue.
    item: Item,
    /// What is actually loaded: `item`, or the music video of a track.
    loading: Item,
    /// Transcode session; `None` for direct play.
    session: Option<String>,
    position: f64,
    last_ui_update: f64,
    last_timeline_update: f64,
    paused: bool,
    started: bool,
    pending_seek: Option<f64>,
    tracks_applied: bool,
    queue_refreshed: bool,
    end_action: EndAction,
}

#[derive(Default)]
struct State {
    queue: Option<PlayQueue>,
    index: usize,
    current: Option<Playing>,
    /// Incremented for every start, so that a slow start that was overtaken
    /// by a newer one can notice and give up.
    generation: u64,
    prefer_music_clips: bool,
    repeat: bool,
}

struct Inner {
    /// Created when something is played for the first time, so that only
    /// browsing does not cost mpv's memory and threads.
    mpv: OnceCell<Arc<Mpv>>,
    /// Called with the new mpv core, before it opens its first file; the
    /// video page connects its renderer here.
    on_mpv_created: RefCell<Option<Box<dyn Fn(Arc<Mpv>)>>>,
    /// The `--deinterlace` option, applied when mpv is created.
    deinterlace: RefCell<Option<String>>,
    settings: gio::Settings,
    state: RefCell<State>,
    listeners: RefCell<Vec<Rc<dyn Fn(&Event)>>>,
}

thread_local! {
    /// Rating key of what is playing, and who wants to know when it changes.
    static NOW_PLAYING: std::cell::Cell<Option<ItemId>> = const { std::cell::Cell::new(None) };
    static NOW_PLAYING_LISTENERS: RefCell<Vec<Box<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
}

/// Rating key of the item that is playing, for lists that mark it.
pub fn now_playing() -> Option<ItemId> {
    NOW_PLAYING.get()
}

/// Calls `listener` whenever another item starts or playback stops.
pub fn connect_now_playing(listener: impl Fn() + 'static) {
    NOW_PLAYING_LISTENERS.with_borrow_mut(|listeners| listeners.push(Box::new(listener)));
}

fn set_now_playing(rating_key: Option<ItemId>) {
    if NOW_PLAYING.replace(rating_key) != rating_key {
        NOW_PLAYING_LISTENERS.with_borrow(|listeners| {
            for listener in listeners {
                listener();
            }
        });
    }
}

thread_local! {
    /// mpv's wakeup callback runs on an mpv thread and cannot carry a
    /// reference to the (main-thread only) player, so it looks it up here.
    static PLAYER: RefCell<Weak<Inner>> = const { RefCell::new(Weak::new()) };
}

#[derive(Clone)]
pub struct Player {
    inner: Rc<Inner>,
}

impl Player {
    pub fn new() -> Self {
        let settings = gio::Settings::new(config::APP_ID);
        let inner = Rc::new(Inner {
            mpv: OnceCell::new(),
            on_mpv_created: RefCell::default(),
            deinterlace: RefCell::default(),
            state: RefCell::new(State {
                prefer_music_clips: settings.boolean("prefer-music-clips"),
                ..State::default()
            }),
            settings,
            listeners: RefCell::default(),
        });
        PLAYER.set(Rc::downgrade(&inner));

        let weak = Rc::downgrade(&inner);
        inner.settings.connect_changed(None, move |settings, key| {
            let Some(inner) = weak.upgrade() else { return };
            match key {
                "volume-level" => inner.apply_volume(),
                "prefer-music-clips" => {
                    inner.state.borrow_mut().prefer_music_clips = settings.boolean(key)
                }
                _ => {}
            }
        });

        let weak = Rc::downgrade(&inner);
        glib::timeout_add_seconds_local(PING_INTERVAL, move || match weak.upgrade() {
            Some(inner) => {
                inner.ping_session();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });

        Self { inner }
    }

    /// Registers what connects a video surface to mpv. It is called once,
    /// when mpv is created for the first playback.
    pub fn connect_mpv_created(&self, callback: impl Fn(Arc<Mpv>) + 'static) {
        self.inner.on_mpv_created.replace(Some(Box::new(callback)));
    }

    /// Registers a callback for everything the player reports.
    pub fn connect(&self, listener: impl Fn(&Event) + 'static) {
        self.inner.listeners.borrow_mut().push(Rc::new(listener));
    }

    // What is playing

    /// The queue entry that is playing, if any.
    pub fn current_item(&self) -> Option<Item> {
        let state = self.inner.state.borrow();
        state.current.as_ref().map(|playing| playing.item.clone())
    }

    /// The loaded item with its audio and subtitle streams.
    pub fn loaded_item(&self) -> Option<Item> {
        let state = self.inner.state.borrow();
        state
            .current
            .as_ref()
            .map(|playing| playing.loading.clone())
    }

    pub fn queue_items(&self) -> Vec<Item> {
        let state = self.inner.state.borrow();
        state
            .queue
            .as_ref()
            .map(|queue| queue.items.clone())
            .unwrap_or_default()
    }

    pub fn queue_index(&self) -> usize {
        self.inner.state.borrow().index
    }

    pub fn is_shuffled(&self) -> bool {
        let state = self.inner.state.borrow();
        state.queue.as_ref().is_some_and(|queue| queue.shuffled)
    }

    pub fn has_next(&self) -> bool {
        let state = self.inner.state.borrow();
        state
            .queue
            .as_ref()
            .is_some_and(|queue| state.index + 1 < queue.items.len())
    }

    pub fn has_previous(&self) -> bool {
        self.inner.state.borrow().index > 0
    }

    pub fn is_playing(&self) -> bool {
        self.inner.state.borrow().current.is_some()
    }

    pub fn is_paused(&self) -> bool {
        let state = self.inner.state.borrow();
        state.current.as_ref().is_some_and(|playing| playing.paused)
    }

    /// Position in milliseconds.
    pub fn position(&self) -> u64 {
        let state = self.inner.state.borrow();
        state
            .current
            .as_ref()
            .map(|playing| (playing.position * 1000.0) as u64)
            .unwrap_or(0)
    }

    // Starting playback

    /// Builds a play queue for `target` on the server and starts it.
    pub fn play(&self, target: PlayTarget, shuffle: bool, mode: StartMode) {
        let inner = self.inner.clone();
        glib::spawn_future_local(async move {
            let Some(server) = Session::current().server() else {
                return;
            };
            let queue = runtime::run(async move {
                match target {
                    PlayTarget::Section { section, sort } => {
                        server
                            .create_section_play_queue(&section, sort.as_deref(), shuffle)
                            .await
                    }
                    PlayTarget::PlaylistItem { playlist, item } => {
                        let (playlist, item) =
                            tokio::try_join!(server.item(playlist), server.item(item))?;
                        server
                            .create_play_queue(&playlist, Some(&item), shuffle)
                            .await
                    }
                    PlayTarget::Item(key) => {
                        let item = server.item(key).await?;
                        // A track plays within its album, so that the rest of
                        // the album follows.
                        match (item.kind, item.parent_rating_key) {
                            (ItemKind::Track, Some(album)) => {
                                let album = server.item(album).await?;
                                server.create_play_queue(&album, Some(&item), shuffle).await
                            }
                            _ => server.create_play_queue(&item, None, shuffle).await,
                        }
                    }
                }
            })
            .await;
            match queue {
                Ok(queue) if queue.items.is_empty() => {
                    crate::notify::message(&crate::i18n::gettext("Nothing to play"))
                }
                Ok(queue) => {
                    let index = queue.selected_index().unwrap_or(0);
                    inner.set_queue(queue, index);
                    inner.start(index, mode);
                }
                Err(err) => crate::notify::failure(
                    &crate::i18n::gettext("Could not create a play queue"),
                    &err,
                ),
            }
        });
    }

    /// Plays offline copies, without involving the server.
    pub fn play_local(&self, items: Vec<Item>) {
        if items.is_empty() {
            return;
        }
        self.inner.set_queue(PlayQueue::local(items), 0);
        self.inner.start(0, StartMode::Ask);
    }

    /// Answers [`Event::AskResume`] for the current queue entry.
    pub fn start_current(&self, mode: StartMode) {
        let index = self.inner.state.borrow().index;
        self.inner.start(index, mode);
    }

    /// Jumps to another entry of the queue.
    pub fn play_index(&self, index: usize) {
        if self.is_playing() {
            self.inner.end_with(EndAction::Index(index));
        } else {
            self.inner.start(index, StartMode::Ask);
        }
    }

    // Controls

    pub fn play_pause(&self) {
        if self.is_playing() {
            let paused = self.is_paused();
            self.inner.set_pause(!paused);
        }
    }

    pub fn pause(&self) {
        if self.is_playing() {
            self.inner.set_pause(true);
        }
    }

    pub fn resume(&self) {
        if self.is_playing() {
            self.inner.set_pause(false);
        }
    }

    pub fn stop(&self) {
        self.inner.end_with(EndAction::Stop);
    }

    pub fn next(&self) {
        self.inner.end_with(EndAction::Advance);
    }

    pub fn previous(&self) {
        self.inner.end_with(EndAction::Previous);
    }

    /// Seeks relative to the current position, in seconds.
    pub fn seek_by(&self, seconds: i32) {
        if self.is_playing() {
            self.inner
                .command("seek", &[&seconds.to_string(), "relative"]);
        }
    }

    /// Seeks to a position in milliseconds.
    pub fn seek_to(&self, position: u64) {
        if self.is_playing() {
            self.inner
                .set_mpv("playback-time", position as f64 / 1000.0);
        }
    }

    pub fn set_repeat(&self, repeat: bool) {
        self.inner
            .set_mpv("loop-file", if repeat { "inf" } else { "no" });
        let changed = std::mem::replace(&mut self.inner.state.borrow_mut().repeat, repeat) != repeat;
        if changed {
            self.inner.emit(Event::RepeatChanged(repeat));
        }
    }

    /// Shuffles or restores the order of the play queue on the server.
    pub fn set_shuffle(&self, shuffle: bool) {
        let inner = self.inner.clone();
        let Some(id) = inner
            .state
            .borrow()
            .queue
            .as_ref()
            .filter(|queue| !queue.is_local())
            .map(|queue| queue.id)
        else {
            return;
        };
        glib::spawn_future_local(async move {
            let Some(server) = Session::current().server() else {
                return;
            };
            match runtime::run(async move { server.shuffle_play_queue(id, shuffle).await }).await {
                Ok(queue) => inner.replace_queue(queue),
                Err(err) => crate::notify::failure(
                    &crate::i18n::gettext("Could not shuffle the play queue"),
                    &err,
                ),
            }
        });
    }

    /// `yes`, `no` or `auto`: whether mpv deinterlaces the video.
    pub fn set_deinterlace(&self, mode: &str) {
        self.inner.deinterlace.replace(Some(mode.to_owned()));
        self.inner.set_mpv("deinterlace", mode);
    }

    /// Ends playback for good, e.g. when the application closes: reports the
    /// final position and releases the transcoder, waiting a few seconds at
    /// most for the server to answer.
    pub async fn shutdown(&self) {
        let Some(playing) = self.inner.state.borrow_mut().current.take() else {
            return;
        };
        self.inner.command("stop", &[]);
        let Some(server) = Session::current().server() else {
            return;
        };
        let mut item = playing.item.clone();
        item.duration = playing.loading.duration.or(item.duration);
        let position = (playing.position * 1000.0) as u64;
        let (loading, session) = (playing.loading, playing.session);
        runtime::run(async move {
            let farewell = async {
                if let Err(err) = server
                    .update_timeline(&item, position, PlaybackState::Stopped, session.as_deref())
                    .await
                {
                    eprintln!("Could not report the playback position: {err}");
                }
                if let Some(session) = &session {
                    if let Err(err) = server.end_stream(&loading, session).await {
                        eprintln!("Could not end the transcode session: {err}");
                    }
                }
            };
            let _ = tokio::time::timeout(Duration::from_secs(3), farewell).await;
        })
        .await;
    }

    /// Selects an audio stream of the loaded item (a Plex stream id).
    pub fn set_audio_stream(&self, stream_id: ItemId) {
        self.inner
            .clone()
            .select_stream(StreamKind::Audio, Some(stream_id));
    }

    /// Selects a subtitle stream of the loaded item, or none.
    pub fn set_subtitle_stream(&self, stream_id: Option<ItemId>) {
        self.inner
            .clone()
            .select_stream(StreamKind::Subtitle, stream_id);
    }
}

impl Inner {
    /// The mpv core, created on first use.
    fn ensure_mpv(&self) -> Result<&Arc<Mpv>, String> {
        if let Some(mpv) = self.mpv.get() {
            return Ok(mpv);
        }
        let mpv = Arc::new(create_mpv().map_err(|err| format!("could not start mpv: {err}"))?);
        if let Some(mode) = self.deinterlace.borrow().as_deref() {
            if let Err(err) = mpv.set_property("deinterlace", mode) {
                eprintln!("Could not set deinterlacing: {err}");
            }
        }
        if let Some(callback) = self.on_mpv_created.borrow().as_ref() {
            callback(mpv.clone());
        }
        Ok(self.mpv.get_or_init(|| mpv))
    }

    fn emit(&self, event: Event) {
        match &event {
            Event::Started { item, .. } => set_now_playing(Some(item.rating_key)),
            Event::Stopped => set_now_playing(None),
            _ => {}
        }
        // Listeners may call back into the player, so no borrow is held.
        let listeners: Vec<_> = self.listeners.borrow().clone();
        for listener in listeners {
            listener(&event);
        }
    }

    fn command(&self, name: &str, args: &[&str]) {
        // Without mpv nothing is playing, so there is nothing to command.
        if let Some(Err(err)) = self.mpv.get().map(|mpv| mpv.command(name, args)) {
            eprintln!("mpv {name} failed: {err}");
        }
    }

    fn set_mpv<T: libmpv2::SetData>(&self, name: &str, value: T) {
        if let Some(Err(err)) = self.mpv.get().map(|mpv| mpv.set_property(name, value)) {
            eprintln!("Could not set mpv property {name}: {err}");
        }
    }

    fn set_pause(&self, pause: bool) {
        self.set_mpv("pause", pause);
    }

    fn apply_volume(&self) {
        self.set_mpv("volume", i64::from(self.settings.int("volume-level")));
    }

    fn set_queue(&self, queue: PlayQueue, index: usize) {
        {
            let mut state = self.state.borrow_mut();
            state.queue = Some(queue);
            state.index = index;
        }
        self.emit(Event::QueueChanged);
    }

    /// Takes over a newer version of the queue, keeping the place in it.
    fn replace_queue(&self, queue: PlayQueue) {
        {
            let mut state = self.state.borrow_mut();
            let playing = state
                .current
                .as_ref()
                .and_then(|playing| playing.item.play_queue_item_id);
            let index = queue
                .items
                .iter()
                .position(|item| playing.is_some() && item.play_queue_item_id == playing)
                .or_else(|| queue.selected_index())
                .unwrap_or(0);
            state.queue = Some(queue);
            state.index = index;
        }
        self.emit(Event::QueueChanged);
    }

    /// Ends the current file; `action` decides what follows.
    fn end_with(&self, action: EndAction) {
        let playing = {
            let mut state = self.state.borrow_mut();
            match state.current.as_mut() {
                Some(playing) => {
                    playing.end_action = action;
                    true
                }
                None => false,
            }
        };
        if playing {
            self.command("stop", &[]);
        }
    }

    /// Starts the queue entry at `index`, replacing whatever is playing.
    fn start(self: &Rc<Self>, index: usize, mode: StartMode) {
        let item = {
            let mut state = self.state.borrow_mut();
            let Some(item) = state
                .queue
                .as_ref()
                .and_then(|queue| queue.items.get(index).cloned())
            else {
                return;
            };
            state.index = index;
            if let Some(playing) = state.current.as_mut() {
                // Continue here once mpv has let go of the current file.
                playing.end_action = EndAction::Replace(mode);
                drop(state);
                self.command("stop", &[]);
                return;
            }
            item
        };

        if mode == StartMode::Ask && item.view_offset != 0 {
            self.emit(Event::AskResume(Box::new(item)));
            return;
        }

        let generation = {
            let mut state = self.state.borrow_mut();
            state.generation += 1;
            state.generation
        };
        let inner = self.clone();
        glib::spawn_future_local(async move {
            if let Err(err) = inner.load(item, mode, generation).await {
                crate::notify::failure(&crate::i18n::gettext("Could not start playback"), &err);
                inner.emit(Event::Stopped);
            }
        });
    }

    async fn load(&self, item: Item, mode: StartMode, generation: u64) -> Result<(), String> {
        let session_state = Session::current();
        let server = session_state.server();
        let machine = session_state.machine_identifier().unwrap_or_default();
        let user = session_state.user_key().unwrap_or_default();
        let prefer_clips = self.state.borrow().prefer_music_clips;

        // The play queue lists items without their streams, and a track may
        // be replaced by its music video. Without a connection the item is
        // used as it was stored with its offline copy.
        let loading = match &server {
            Some(server) => {
                let key = match (prefer_clips, item.kind, item.primary_extra_key.as_deref()) {
                    (true, ItemKind::Track, Some(clip)) => {
                        crate::util::rating_key_from_path(clip).unwrap_or(item.rating_key)
                    }
                    _ => item.rating_key,
                };
                let server = server.clone();
                runtime::run(async move { server.item(key).await })
                    .await
                    .map_err(|err| err.to_string())?
            }
            None => item.clone(),
        };

        let direct = self.settings.boolean("play-media-direct") || loading.kind.is_audio();
        let opened = if let Some(path) =
            crate::sync::local_file(&machine, &user, loading.rating_key)
        {
            // An offline copy plays without the server.
            Opened {
                url: path.to_string_lossy().into_owned(),
                session: None,
                subtitle_delay: 0.0,
            }
        } else {
            let server = server.ok_or("not connected to a server")?;
            let options = StreamOptions {
                direct,
                resolution: Some(
                    self.settings
                        .string("transcode-media-to-resolution")
                        .to_string(),
                ),
            };
            let loading = loading.clone();
            runtime::run(async move { server.open_stream(&loading, &options).await })
                .await
                .map_err(|err| match err {
                    girens_model::Error::NotFound(_) => "the item has no media file".to_owned(),
                    err => err.to_string(),
                })?
        };
        let Opened {
            url,
            session,
            subtitle_delay,
        } = opened;

        if self.state.borrow().generation != generation {
            // Another start overtook this one.
            return Ok(());
        }

        let offset = match mode {
            StartMode::At(position) => position as f64 / 1000.0,
            StartMode::Resume => loading.view_offset as f64 / 1000.0,
            StartMode::Ask | StartMode::Beginning => 0.0,
        };
        self.state.borrow_mut().current = Some(Playing {
            item,
            loading,
            session,
            position: 0.0,
            last_ui_update: 0.0,
            last_timeline_update: 0.0,
            paused: true,
            started: false,
            pending_seek: (offset > 0.0).then_some(offset),
            tracks_applied: false,
            queue_refreshed: false,
            end_action: EndAction::Advance,
        });

        let mpv = self.ensure_mpv()?;
        self.set_mpv("sub-delay", subtitle_delay);
        self.apply_volume();
        // Stay paused until mpv is ready and the start position is set.
        self.set_pause(true);
        mpv.command("loadfile", &[url.as_str(), "replace"])
            .map_err(|err| format!("mpv could not open the stream: {err}"))
    }

    // mpv events

    fn drain_events(self: &Rc<Self>) {
        let Some(mpv) = self.mpv.get() else {
            return;
        };
        while let Some(event) = mpv.wait_event(0.0) {
            match event {
                Ok(MpvEvent::PropertyChange { name, change, .. }) => match (name, change) {
                    ("time-pos", PropertyData::Double(position)) => self.on_position(position),
                    ("pause", PropertyData::Flag(paused)) => self.on_pause(paused),
                    ("eof-reached", PropertyData::Flag(true)) => self.command("stop", &[]),
                    ("track-list/count", PropertyData::Int64(count)) if count > 0 => {
                        self.on_tracks_available()
                    }
                    _ => {}
                },
                Ok(MpvEvent::PlaybackRestart) => self.on_playback_restart(),
                Ok(MpvEvent::EndFile(reason)) => {
                    self.on_end_file(format!("{reason:?}").contains("Error"))
                }
                Ok(_) => {}
                Err(err) => eprintln!("mpv error: {err}"),
            }
        }
    }

    fn on_playback_restart(&self) {
        let (seek, started) = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.current.as_mut() else {
                return;
            };
            if let Some(seek) = playing.pending_seek.take() {
                (Some(seek), None)
            } else if !playing.started {
                playing.started = true;
                (None, Some(playing.item.clone()))
            } else {
                (None, None)
            }
        };
        if let Some(seek) = seek {
            // Triggers another playback-restart, which then starts playback.
            self.set_mpv("playback-time", seek);
        } else if let Some(item) = started {
            let video = self
                .state
                .borrow()
                .current
                .as_ref()
                .is_some_and(|playing| !playing.loading.kind.is_audio());
            self.set_pause(false);
            self.emit(Event::Started {
                item: Box::new(item),
                video,
            });
        }
    }

    fn on_position(self: &Rc<Self>, position: f64) {
        let (ui, timeline, refresh_queue) = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.current.as_mut() else {
                return;
            };
            playing.position = position;
            let ui = playing.started && (position - playing.last_ui_update).abs() > UI_INTERVAL;
            if ui {
                playing.last_ui_update = position;
            }
            let timeline = playing.started
                && (position - playing.last_timeline_update).abs() > TIMELINE_INTERVAL;
            if timeline {
                playing.last_timeline_update = position;
            }
            // Once playback is really under way the server may have extended
            // the queue (the next episodes of a show).
            let refresh_queue = playing.started && position > 1.0 && !playing.queue_refreshed;
            if refresh_queue {
                playing.queue_refreshed = true;
            }
            (ui, timeline, refresh_queue)
        };
        if ui {
            self.emit(Event::Position((position * 1000.0) as u64));
        }
        if timeline {
            self.report_timeline(PlaybackState::Playing);
        }
        if refresh_queue {
            self.refresh_queue();
        }
    }

    fn on_pause(&self, paused: bool) {
        let started = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.current.as_mut() else {
                return;
            };
            playing.paused = paused;
            playing.started
        };
        if started {
            self.emit(Event::Paused(paused));
            if paused {
                self.report_timeline(PlaybackState::Paused);
            }
        }
    }

    fn on_end_file(self: &Rc<Self>, failed: bool) {
        let Some(playing) = self.state.borrow_mut().current.take() else {
            return;
        };
        self.send_timeline(&playing, PlaybackState::Stopped);
        self.end_transcode_session(&playing);

        let (index, length) = {
            let state = self.state.borrow();
            (
                state.index,
                state.queue.as_ref().map_or(0, |queue| queue.items.len()),
            )
        };
        let next = next_after(playing.end_action, index, length, failed);
        match next {
            Some((index, mode)) => {
                self.emit(Event::QueueChanged);
                self.start(index, mode);
            }
            None => self.emit(Event::Stopped),
        }
    }

    // Reporting to the server

    fn report_timeline(&self, playback: PlaybackState) {
        let state = self.state.borrow();
        if let Some(playing) = state.current.as_ref() {
            self.send_timeline(playing, playback);
        }
    }

    fn send_timeline(&self, playing: &Playing, playback: PlaybackState) {
        let session_state = Session::current();
        // Progress is reported for the queue entry, with the duration of what
        // is actually playing.
        let mut item = playing.item.clone();
        item.duration = playing.loading.duration.or(item.duration);
        let position = (playing.position * 1000.0) as u64;

        // An offline copy of a video remembers the position itself, for
        // when the server cannot be asked.
        let key = item.rating_key;
        let copy = match (
            matches!(item.kind, ItemKind::Movie | ItemKind::Episode),
            session_state.machine_identifier(),
            session_state.user_key(),
        ) {
            (true, Some(machine), Some(user)) => {
                crate::sync::record_progress(&machine, &user, key, position, item.duration);
                Some((machine, user))
            }
            _ => None,
        };

        let Some(server) = session_state.server() else {
            return;
        };
        let session = playing.session.clone();
        glib::spawn_future_local(async move {
            let result = runtime::run(async move {
                server
                    .update_timeline(&item, position, playback, session.as_deref())
                    .await
            })
            .await;
            match (result, copy) {
                (Ok(()), Some((machine, user))) => {
                    crate::sync::progress_sent(&machine, &user, key);
                }
                (Ok(()), None) => {}
                (Err(err), _) => eprintln!("Could not report the playback position: {err}"),
            }
        });
    }

    /// Tells the server it can stop transcoding for what was playing.
    fn end_transcode_session(&self, playing: &Playing) {
        let (Some(server), Some(session)) = (Session::current().server(), playing.session.clone())
        else {
            return;
        };
        let loading = playing.loading.clone();
        glib::spawn_future_local(async move {
            let result =
                runtime::run(
                    async move { server.end_stream(&loading, &session).await },
                )
                .await;
            if let Err(err) = result {
                eprintln!("Could not end the transcode session: {err}");
            }
        });
    }

    fn ping_session(&self) {
        let (Some(server), Some((loading, session))) = (
            Session::current().server(),
            self.state.borrow().current.as_ref().and_then(|playing| {
                playing
                    .session
                    .clone()
                    .map(|session| (playing.loading.clone(), session))
            }),
        ) else {
            return;
        };
        glib::spawn_future_local(async move {
            let result =
                runtime::run(
                    async move { server.keep_stream(&loading, &session).await },
                )
                .await;
            if let Err(err) = result {
                eprintln!("Could not keep the transcode session alive: {err}");
            }
        });
    }

    fn refresh_queue(self: &Rc<Self>) {
        let (Some(server), Some(id)) = (
            Session::current().server(),
            self.state
                .borrow()
                .queue
                .as_ref()
                .filter(|queue| !queue.is_local())
                .map(|queue| queue.id),
        ) else {
            return;
        };
        let inner = self.clone();
        glib::spawn_future_local(async move {
            match runtime::run(async move { server.refresh_play_queue(id).await }).await {
                Ok(queue) => inner.replace_queue(queue),
                Err(err) => eprintln!("Could not refresh the play queue: {err}"),
            }
        });
    }

    // Audio and subtitle streams

    fn on_tracks_available(&self) {
        let apply = {
            let mut state = self.state.borrow_mut();
            match state.current.as_mut() {
                // A transcoded stream already contains only what was selected.
                Some(playing)
                    if playing.session.is_none()
                        && !playing.loading.kind.is_audio()
                        && !playing.tracks_applied =>
                {
                    playing.tracks_applied = true;
                    true
                }
                _ => false,
            }
        };
        if apply {
            self.apply_streams();
        }
    }

    /// Makes mpv use the streams that are selected on the server.
    fn apply_streams(&self) {
        let (Some(server), Some(loading)) = (
            Session::current().server(),
            self.state
                .borrow()
                .current
                .as_ref()
                .map(|playing| playing.loading.clone()),
        ) else {
            return;
        };
        let tracks = self.mpv_tracks();
        let find = |kind: &str, matches: &dyn Fn(&MpvTrack) -> bool| {
            tracks
                .iter()
                .find(|track| track.kind == kind && matches(track))
                .map(|track| track.id)
        };

        if let Some(stream) = loading.selected_stream(StreamKind::Audio) {
            let index = stream.index.map(i64::from);
            if let Some(id) = find("audio", &|track| index.is_some() && track.ff_index == index) {
                self.set_mpv("aid", id);
            }
        }

        match loading.selected_stream(StreamKind::Subtitle) {
            None => self.set_mpv("sid", "no"),
            Some(stream) => match stream.key.as_deref() {
                // A sidecar file: mpv loads it from the server, titled with
                // the stream id so that it is recognised next time.
                Some(key) => {
                    let title = stream.id.to_string();
                    match find("sub", &|track| track.title.as_deref() == Some(&title)) {
                        Some(id) => self.set_mpv("sid", id),
                        None => self.command(
                            "sub-add",
                            &[&server.download_url(key), "select", &title],
                        ),
                    }
                }
                None => {
                    let index = stream.index.map(i64::from);
                    if let Some(id) =
                        find("sub", &|track| index.is_some() && track.ff_index == index)
                    {
                        self.set_mpv("sid", id);
                    }
                }
            },
        }
    }

    fn mpv_tracks(&self) -> Vec<MpvTrack> {
        let Some(mpv) = self.mpv.get() else {
            return Vec::new();
        };
        let count = mpv.get_property::<i64>("track-list/count").unwrap_or(0);
        (0..count)
            .filter_map(|index| {
                let property = |name: &str| format!("track-list/{index}/{name}");
                Some(MpvTrack {
                    kind: mpv.get_property::<String>(&property("type")).ok()?,
                    id: mpv.get_property::<i64>(&property("id")).ok()?,
                    ff_index: mpv.get_property::<i64>(&property("ff-index")).ok(),
                    title: mpv.get_property::<String>(&property("title")).ok(),
                })
            })
            .collect()
    }

    /// Stores the choice on the server and applies it: directly for direct
    /// play, by restarting the stream when transcoding.
    fn select_stream(self: Rc<Self>, kind: StreamKind, stream_id: Option<ItemId>) {
        let (Some(server), Some(loading)) = (
            Session::current().server(),
            self.state
                .borrow()
                .current
                .as_ref()
                .map(|playing| playing.loading.clone()),
        ) else {
            return;
        };
        let Some(part) = loading.parts().next().cloned() else {
            return;
        };
        glib::spawn_future_local(async move {
            let key = loading.rating_key;
            let updated = runtime::run(async move {
                match (kind, stream_id) {
                    (StreamKind::Audio, Some(id)) => server.select_audio_stream(&part, id).await?,
                    (StreamKind::Subtitle, id) => server.select_subtitle_stream(&part, id).await?,
                    _ => {}
                }
                server.item(key).await
            })
            .await;
            let updated = match updated {
                Ok(updated) => updated,
                Err(err) => {
                    crate::notify::failure(
                        &crate::i18n::gettext("Could not change the stream"),
                        &err,
                    );
                    return;
                }
            };

            let transcoding = {
                let mut state = self.state.borrow_mut();
                match state.current.as_mut() {
                    Some(playing) if playing.loading.rating_key == key => {
                        playing.loading = updated;
                        Some((playing.session.is_some(), playing.position))
                    }
                    _ => None,
                }
            };
            match transcoding {
                Some((true, position)) => {
                    self.end_with(EndAction::Restart((position * 1000.0) as u64))
                }
                Some((false, _)) => self.apply_streams(),
                None => {}
            }
        });
    }
}

/// Creates the mpv core and makes it wake the main loop for its events.
fn create_mpv() -> Result<Mpv, libmpv2::Error> {
    let mut mpv = Mpv::with_initializer(|init| {
        init.set_property("vo", "libmpv")?;
        init.set_property("keep-open", "yes")?;
        init.set_property("cache", "yes")?;
        // Decode on the GPU where that is known to work, else in software.
        init.set_property("hwdec", "auto-safe")?;
        // `GIRENS_MPV_LOG=v` (or any mpv message level) prints mpv's log.
        if let Ok(level) = std::env::var("GIRENS_MPV_LOG") {
            init.set_property("terminal", "yes")?;
            init.set_property("msg-level", format!("all={level}").as_str())?;
        }
        Ok(())
    })?;
    mpv.disable_deprecated_events()?;
    mpv.observe_property("time-pos", Format::Double, 1)?;
    mpv.observe_property("pause", Format::Flag, 2)?;
    mpv.observe_property("eof-reached", Format::Flag, 3)?;
    mpv.observe_property("track-list/count", Format::Int64, 4)?;

    let pending = Arc::new(AtomicBool::new(false));
    mpv.set_wakeup_callback(move || {
        if pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let pending = pending.clone();
        glib::MainContext::default().invoke(move || {
            pending.store(false, Ordering::Release);
            if let Some(inner) = PLAYER.with_borrow(Weak::upgrade) {
                inner.drain_events();
            }
        });
    });
    Ok(mpv)
}

struct MpvTrack {
    kind: String,
    id: i64,
    ff_index: Option<i64>,
    title: Option<String>,
}

/// A fixed delay before the cursor and controls hide, shared by the views.
pub const CONTROLS_HIDE_DELAY: Duration = Duration::from_secs(3);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_item_is_followed_by_the_next_until_the_queue_ends() {
        assert_eq!(
            next_after(EndAction::Advance, 0, 3, false),
            Some((1, StartMode::Ask))
        );
        assert_eq!(next_after(EndAction::Advance, 2, 3, false), None);
    }

    #[test]
    fn a_failed_item_does_not_run_through_the_queue() {
        assert_eq!(next_after(EndAction::Advance, 0, 3, true), None);
    }

    #[test]
    fn previous_stops_at_the_start() {
        assert_eq!(
            next_after(EndAction::Previous, 2, 3, false),
            Some((1, StartMode::Ask))
        );
        assert_eq!(next_after(EndAction::Previous, 0, 3, false), None);
    }

    #[test]
    fn jumping_needs_an_existing_entry() {
        assert_eq!(
            next_after(EndAction::Index(2), 0, 3, false),
            Some((2, StartMode::Ask))
        );
        assert_eq!(next_after(EndAction::Index(3), 0, 3, false), None);
    }

    #[test]
    fn restart_and_replace_keep_their_start_position() {
        assert_eq!(
            next_after(EndAction::Restart(42_000), 1, 3, false),
            Some((1, StartMode::At(42_000)))
        );
        assert_eq!(
            next_after(EndAction::Replace(StartMode::Beginning), 1, 3, false),
            Some((1, StartMode::Beginning))
        );
        // A failure of the old file does not cancel what was requested next.
        assert_eq!(
            next_after(EndAction::Replace(StartMode::Resume), 1, 3, true),
            Some((1, StartMode::Resume))
        );
    }

    #[test]
    fn stop_ends_playback() {
        assert_eq!(next_after(EndAction::Stop, 0, 3, false), None);
    }
}
