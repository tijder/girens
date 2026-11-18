//! Media playback with mpv.
//!
//! Everything runs on the main thread: mpv wakes
//! the main loop when it has events, and what happens when a file ends is
//! decided by an explicit [`EndAction`] instead of a set of flags.

pub mod render;
mod reporting;
mod streams;

use girens_model::ItemId;
use std::cell::{OnceCell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use girens_model::model::{Item, ItemKind, LibrarySection, Segment, SegmentKind, StreamKind};
use girens_model::subtitles::{Cue, PictureSubtitles};
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
/// Has FFmpeg connect again when a stream breaks off, instead of ending
/// playback as if the file were over.
const RECONNECT: &str =
    "reconnect=1,reconnect_streamed=1,reconnect_on_network_error=1,reconnect_delay_max=30";
/// Seconds an intro plays before it is skipped for a user who wants that.
const AUTO_SKIP_DELAY: f64 = 5.0;
/// A file has to go on for this long after an episode, in seconds, for the
/// episode to be taken as a part of it.
const SLICE_MARGIN: f64 = 2.0;

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
    /// Playback entered the intro or the credits, or left them.
    Segment(Option<SegmentKind>),
    /// Another audio or subtitle stream of the loaded item was chosen.
    StreamsChanged,
    /// The subtitles to draw over the video changed; `None` when there
    /// are none to draw.
    Subtitles(Option<SubtitlePictures>),
    /// Whether the item that is playing starts over when it ends.
    RepeatChanged(bool),
    /// The playback speed, 1.0 being the normal one.
    SpeedChanged(f64),
    /// Nothing is playing any more.
    Stopped,
}

/// Subtitles that are pictures, as they are on screen at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitlePictures {
    /// The size of the frame the cues are placed on.
    pub canvas: (f32, f32),
    /// Width over height of the video as it is shown, which need not be
    /// that of the canvas (a DVD has pixels that are not square).
    pub aspect: f64,
    /// Each picture with where to get the sheet it is cut out of.
    pub cues: Vec<(String, Cue)>,
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
    /// Seconds into the item, which is what the user sees.
    position: f64,
    /// Where the item starts in the file mpv plays, in seconds.
    base: f64,
    /// The intro or credits playback is in.
    segment: Option<Segment>,
    /// Whether the intro was skipped for the user already.
    intro_skipped: bool,
    /// The subtitles the player draws itself, of the stream with this id.
    pictures: Option<(ItemId, Rc<PictureSubtitles>)>,
    /// Those on screen now.
    shown: Vec<Cue>,
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
    /// The file the user chose for an item that comes in several.
    version: Option<(ItemId, ItemId)>,
    queue: Option<PlayQueue>,
    index: usize,
    current: Option<Playing>,
    /// Incremented for every start, so that a slow start that was overtaken
    /// by a newer one can notice and give up.
    generation: u64,
    prefer_music_clips: bool,
    repeat: bool,
    speed: f64,
    muted: bool,
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

/// What pages show of the playback: which row to mark and which play button
/// is a pause button for the time being.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct NowPlaying {
    /// Rating key of the item that is playing.
    item: Option<ItemId>,
    /// Rating key of the album, artist or playlist the queue was made of.
    source: Option<ItemId>,
    paused: bool,
}

thread_local! {
    /// What is playing, and who wants to know when it changes.
    static NOW_PLAYING: std::cell::Cell<NowPlaying> = const {
        std::cell::Cell::new(NowPlaying { item: None, source: None, paused: false })
    };
    static NOW_PLAYING_LISTENERS: RefCell<Vec<Box<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
}

/// Rating key of the item that is playing, for lists that mark it.
pub fn now_playing() -> Option<ItemId> {
    NOW_PLAYING.get().item
}

/// Whether what is playing is paused.
pub fn now_paused() -> bool {
    NOW_PLAYING.get().paused
}

/// Rating key of the album, artist or playlist that is playing: what the
/// user asked for, of which [`now_playing`] is one item.
pub fn playing_source() -> Option<ItemId> {
    let playing = NOW_PLAYING.get();
    playing.item.and(playing.source)
}

/// Whether something plays that more can be queued after.
pub fn can_enqueue() -> bool {
    PLAYER
        .with_borrow(Weak::upgrade)
        .is_some_and(|inner| inner.server_queue().is_some())
}

/// Calls `listener` whenever another item starts, or playback pauses, goes
/// on or stops.
pub fn connect_now_playing(listener: impl Fn() + 'static) {
    NOW_PLAYING_LISTENERS.with_borrow_mut(|listeners| listeners.push(Box::new(listener)));
}

fn update_now_playing(change: impl FnOnce(&mut NowPlaying)) {
    let mut playing = NOW_PLAYING.get();
    change(&mut playing);
    if NOW_PLAYING.replace(playing) != playing {
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
                speed: 1.0,
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
                inner.report_when_paused();
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
                // Each with what the queue is made of, for the page of that
                // to tell that it is playing.
                match target {
                    PlayTarget::Section { section, sort } => server
                        .create_section_play_queue(&section, sort.as_deref(), shuffle)
                        .await
                        .map(|queue| (queue, None)),
                    PlayTarget::PlaylistItem { playlist, item } => {
                        let (playlist, item) =
                            tokio::try_join!(server.item(playlist), server.item(item))?;
                        server
                            .create_play_queue(&playlist, Some(&item), shuffle)
                            .await
                            .map(|queue| (queue, Some(playlist.rating_key)))
                    }
                    PlayTarget::Item(key) => {
                        let item = server.item(key).await?;
                        // A track plays within its album, so that the rest of
                        // the album follows.
                        match (item.kind, item.parent_rating_key) {
                            (ItemKind::Track, Some(album)) => {
                                let album = server.item(album).await?;
                                server
                                    .create_play_queue(&album, Some(&item), shuffle)
                                    .await
                                    .map(|queue| (queue, Some(album.rating_key)))
                            }
                            _ => server
                                .create_play_queue(&item, None, shuffle)
                                .await
                                .map(|queue| (queue, Some(key))),
                        }
                    }
                }
            })
            .await;
            match queue {
                Ok((queue, _)) if queue.items.is_empty() => {
                    crate::notify::message(&crate::i18n::gettext("Nothing to play"))
                }
                Ok((queue, source)) => {
                    let index = queue.selected_index().unwrap_or(0);
                    inner.set_queue(queue, index, source);
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
        self.inner.set_queue(PlayQueue::local(items), 0, None);
        self.inner.start(0, StartMode::Ask);
    }

    /// Adds the item with this rating key to the queue that is playing:
    /// right after what plays now with `next`, at the end otherwise.
    pub fn enqueue(&self, key: ItemId, next: bool) {
        let inner = self.inner.clone();
        let (Some(server), Some((queue, playing))) =
            (Session::current().server(), inner.server_queue())
        else {
            return;
        };
        glib::spawn_future_local(async move {
            let after = playing.filter(|_| next);
            let added = runtime::run(async move {
                let item = server.item(key).await?;
                server.add_to_play_queue(queue, &item, after).await
            })
            .await;
            match added {
                Ok(queue) => {
                    // Something else may have been started in the meantime.
                    let current = inner.server_queue().map(|(id, _)| id);
                    if current == Some(queue.id) {
                        inner.replace_queue(queue);
                    }
                    crate::notify::message(&if next {
                        crate::i18n::gettext("Plays next")
                    } else {
                        crate::i18n::gettext("Added to the play queue")
                    });
                }
                Err(err) => crate::notify::failure(
                    &crate::i18n::gettext("Could not add to the play queue"),
                    &err,
                ),
            }
        });
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
        let base = self
            .inner
            .state
            .borrow()
            .current
            .as_ref()
            .map(|playing| playing.base);
        if let Some(base) = base {
            self.inner
                .set_mpv("playback-time", base + position as f64 / 1000.0);
        }
    }

    /// Jumps past the intro that is playing, or from the credits to the
    /// next item.
    pub fn skip_segment(&self) {
        let segment = self
            .inner
            .state
            .borrow()
            .current
            .as_ref()
            .and_then(|playing| playing.segment);
        match segment {
            Some(segment) if segment.kind == SegmentKind::Outro && self.has_next() => self.next(),
            Some(segment) => self
                .inner
                .set_mpv("playback-time", segment.end as f64 / 1000.0),
            None => {}
        }
    }

    /// The chapters of the loaded item, in milliseconds into it.
    pub fn chapters(&self) -> Vec<girens_model::model::Chapter> {
        self.inner
            .state
            .borrow()
            .current
            .as_ref()
            .map(|playing| playing.loading.chapters.clone())
            .unwrap_or_default()
    }

    /// Jumps to the start of the chapter `steps` further on (or back, when
    /// negative); backwards first goes to the start of the current one.
    pub fn seek_chapter(&self, steps: i32) {
        let chapters = self.chapters();
        if chapters.is_empty() {
            return;
        }
        let position = self.position();
        // Leeway so that "previous" from just after a start goes further back.
        let current = chapters
            .iter()
            .rposition(|chapter| chapter.start_time_offset + 2000 <= position)
            .map_or(-1, |index| index as i32);
        let target = (current + steps).clamp(0, chapters.len() as i32 - 1);
        self.seek_to(chapters[target as usize].start_time_offset);
    }

    pub fn speed(&self) -> f64 {
        self.inner.state.borrow().speed
    }

    /// Plays at `speed` times the normal pace, between 0.5 and 2.
    pub fn set_speed(&self, speed: f64) {
        let speed = (speed * 100.0).round() / 100.0;
        let speed = speed.clamp(0.5, 2.0);
        self.inner.set_mpv("speed", speed);
        let changed = std::mem::replace(&mut self.inner.state.borrow_mut().speed, speed) != speed;
        if changed {
            self.inner.emit(Event::SpeedChanged(speed));
        }
    }

    pub fn is_muted(&self) -> bool {
        self.inner.state.borrow().muted
    }

    pub fn set_muted(&self, muted: bool) {
        self.inner.set_mpv("mute", muted);
        self.inner.state.borrow_mut().muted = muted;
    }

    /// Sets the volume in percent, right away in mpv and remembered in
    /// the settings, which the volume buttons follow.
    pub fn set_volume(&self, volume: i32) {
        let volume = volume.clamp(0, 100);
        self.inner.set_mpv("volume", i64::from(volume));
        if let Err(err) = self.inner.settings.set_int("volume-level", volume) {
            log::warn!("Could not save the volume: {err}");
        }
    }

    pub fn volume(&self) -> i32 {
        self.inner.settings.int("volume-level")
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
        let position = ((playing.position + playing.base) * 1000.0) as u64;
        let (loading, session) = (playing.loading, playing.session);
        runtime::run(async move {
            let farewell = async {
                if let Err(err) = server
                    .update_timeline(&item, position, PlaybackState::Stopped, session.as_deref())
                    .await
                {
                    log::warn!("Could not report the playback position: {err}");
                }
                if let Some(session) = &session {
                    if let Err(err) = server.end_stream(&loading, session).await {
                        log::warn!("Could not end the transcode session: {err}");
                    }
                }
            };
            let _ = tokio::time::timeout(Duration::from_secs(3), farewell).await;
        })
        .await;
    }

    /// Plays another of the files the loaded item comes in, from where
    /// playback is now.
    pub fn set_version(&self, media: ItemId) {
        let position = {
            let mut state = self.inner.state.borrow_mut();
            let Some((item, position)) = state
                .current
                .as_ref()
                .map(|playing| (playing.loading.rating_key, playing.position))
            else {
                return;
            };
            state.version = Some((item, media));
            position
        };
        self.inner
            .end_with(EndAction::Restart((position * 1000.0) as u64));
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
                log::warn!("Could not set deinterlacing: {err}");
            }
        }
        if let Some(callback) = self.on_mpv_created.borrow().as_ref() {
            callback(mpv.clone());
        }
        Ok(self.mpv.get_or_init(|| mpv))
    }

    fn emit(&self, event: Event) {
        match &event {
            Event::Started { item, .. } => update_now_playing(|playing| {
                playing.item = Some(item.rating_key);
                playing.paused = false;
            }),
            Event::Paused(paused) => update_now_playing(|playing| playing.paused = *paused),
            Event::Stopped => update_now_playing(|playing| {
                playing.item = None;
                playing.paused = false;
            }),
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
            log::warn!("mpv {name} failed: {err}");
        }
    }

    fn set_mpv<T: libmpv2::SetData>(&self, name: &str, value: T) {
        if let Some(Err(err)) = self.mpv.get().map(|mpv| mpv.set_property(name, value)) {
            log::warn!("Could not set mpv property {name}: {err}");
        }
    }

    fn set_pause(&self, pause: bool) {
        self.set_mpv("pause", pause);
    }

    fn apply_volume(&self) {
        self.set_mpv("volume", i64::from(self.settings.int("volume-level")));
    }

    /// The id of the queue and of the entry that is playing, when something
    /// plays from a queue the server keeps.
    fn server_queue(&self) -> Option<(ItemId, Option<ItemId>)> {
        let state = self.state.borrow();
        let playing = state.current.as_ref()?;
        let queue = state.queue.as_ref().filter(|queue| !queue.is_local())?;
        Some((queue.id, playing.item.play_queue_item_id))
    }

    /// Starts over with `queue`, made of the album, artist or playlist
    /// `source`.
    fn set_queue(&self, queue: PlayQueue, index: usize, source: Option<ItemId>) {
        update_now_playing(|playing| playing.source = source);
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
        let mut loading = loading;
        let direct = self.settings.boolean("play-media-direct") || loading.kind.is_audio();
        self.prefer_version(&mut loading);

        let mut local_subtitles = Vec::new();
        let opened = if let Some(path) =
            crate::sync::local_file(&machine, &user, loading.rating_key)
        {
            // An offline copy plays without the server. The copy of a
            // stream is a folder, with its subtitles beside the list.
            let stream = path.is_dir();
            if stream {
                local_subtitles = subtitles_in(&path);
            }
            Opened {
                url: if stream {
                    path.join(girens_ister::download::MASTER)
                } else {
                    path
                }
                .to_string_lossy()
                .into_owned(),
                session: None,
                subtitle_delay: if stream {
                    girens_ister::SUBTITLE_DELAY
                } else {
                    0.0
                },
                segmented: false,
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
            segmented,
        } = opened;

        if self.state.borrow().generation != generation {
            // Another start overtook this one.
            return Ok(());
        }

        let base = loading.start_offset as f64 / 1000.0;
        let offset = base
            + match mode {
                StartMode::At(position) => position as f64 / 1000.0,
                StartMode::Resume => loading.view_offset as f64 / 1000.0,
                StartMode::Ask | StartMode::Beginning => 0.0,
            };
        self.state.borrow_mut().current = Some(Playing {
            item,
            loading,
            session,
            position: 0.0,
            base,
            segment: None,
            intro_skipped: false,
            pictures: None,
            shown: Vec::new(),
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
        // A stream in many short files asks again for a file that failed,
        // over a new connection each time: a kept one that went stale would
        // fail them all.
        let demuxer = if segmented {
            format!("{RECONNECT},seg_max_retry=5,http_persistent=0")
        } else {
            RECONNECT.to_owned()
        };
        self.set_mpv("demuxer-lavf-o", demuxer.as_str());
        self.apply_volume();
        let (speed, muted) = {
            let state = self.state.borrow();
            (state.speed, state.muted)
        };
        self.set_mpv("speed", speed);
        self.set_mpv("mute", muted);
        // Stay paused until mpv is ready and the start position is set.
        self.set_pause(true);
        mpv.command("loadfile", &[url.as_str(), "replace"])
            .map_err(|err| format!("mpv could not open the stream: {err}"))?;
        // Offered, not shown: which to show is up to the user.
        for (path, language) in local_subtitles {
            let path = path.to_string_lossy();
            self.command("sub-add", &[&path, "auto", &language, &language]);
        }
        Ok(())
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
                Ok(MpvEvent::EndFile(_)) => self.on_end_file(false),
                Ok(_) => {}
                // A file that ends because of an error comes as that error,
                // not as an end.
                Err(libmpv2::Error::Raw(code)) => self.on_failure(code),
                Err(err) => log::warn!("mpv error: {err}"),
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

    /// What plays is the first file of an item: puts the one the user chose
    /// for `item` there, or else one that plays as it is before those the
    /// server has to convert anyway.
    fn prefer_version(&self, item: &mut Item) {
        if self.settings.boolean("play-media-direct") {
            if let Some(server) = Session::current().server() {
                let fitting = item.media.iter().position(|media| server.plays_as_it_is(media));
                if let Some(fitting) = fitting {
                    item.media[..=fitting].rotate_right(1);
                }
            }
        }
        let chosen = self
            .state
            .borrow()
            .version
            .filter(|(of, _)| *of == item.rating_key)
            .and_then(|(_, media)| item.media.iter().position(|file| file.id == media));
        if let Some(chosen) = chosen {
            item.media.swap(0, chosen);
        }
    }

    /// `time` is where mpv is in the file, in seconds.
    fn on_position(self: &Rc<Self>, time: f64) {
        let (position, slice_end, segment, auto_skip) = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.current.as_mut() else {
                return;
            };
            let position = item_time(time, playing.base);
            // Where the item ends, when it is followed by another in the
            // same file.
            let slice_end = playing
                .loading
                .duration
                .map(|duration| playing.base + duration as f64 / 1000.0)
                .filter(|end| playing.started && time >= *end);
            let now = segment_at(&playing.loading.segments, time);
            let changed = (now != playing.segment).then_some(now);
            playing.segment = now;
            let auto_skip = now.filter(|segment| {
                segment.kind == SegmentKind::Intro
                    && !playing.intro_skipped
                    && time >= segment.start as f64 / 1000.0 + AUTO_SKIP_DELAY
            });
            (position, slice_end, changed, auto_skip)
        };
        self.show_pictures(time);
        if let Some(segment) = segment {
            self.emit(Event::Segment(segment.map(|segment| segment.kind)));
        }
        if let Some(intro) = auto_skip {
            let wanted = Session::current()
                .server()
                .is_some_and(|server| server.skips_intros());
            if let Some(playing) = self.state.borrow_mut().current.as_mut() {
                // Once: who goes back into the intro wants to see it.
                playing.intro_skipped = true;
            }
            if wanted {
                self.set_mpv("playback-time", intro.end as f64 / 1000.0);
                return;
            }
        }
        if let Some(end) = slice_end {
            let file_ends = self
                .mpv
                .get()
                .and_then(|mpv| mpv.get_property::<f64>("duration").ok());
            if file_ends.is_some_and(|file_ends| file_ends > end + SLICE_MARGIN) {
                // The next episode of the file is not this item.
                self.end_with(EndAction::Advance);
                return;
            }
        }

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

    /// mpv gave up on the file: it could not be opened, or broke off for
    /// good.
    fn on_failure(self: &Rc<Self>, code: libmpv2::MpvError) {
        let started = match self.state.borrow().current.as_ref() {
            Some(playing) => playing.started,
            None => {
                log::warn!("mpv error {code}");
                return;
            }
        };
        if started {
            log::warn!("Playback broke off: mpv error {code}");
        } else {
            let detail = format!("mpv error {code}");
            crate::notify::failure(&crate::i18n::gettext("Could not start playback"), &detail);
        }
        self.on_end_file(true);
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

    // The queue

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
                Err(err) => log::warn!("Could not refresh the play queue: {err}"),
            }
        });
    }
}

/// The subtitle files in the folder of a copy, each with its language as
/// the name of the file tells it (`sub_<language>_<id>.srt`).
fn subtitles_in(dir: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    let mut found: Vec<(std::path::PathBuf, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|file| {
            let name = file.file_name().to_str()?.to_owned();
            let language = name.strip_prefix("sub_")?.strip_suffix(".srt")?;
            let language = language.split('_').next()?.to_owned();
            Some((file.path(), language))
        })
        .collect();
    found.sort();
    found
}

/// Seconds into an item for a time in the file it starts at `base` in.
fn item_time(time: f64, base: f64) -> f64 {
    (time - base).max(0.0)
}

/// The intro or credits that `time`, in seconds into the file, falls in.
fn segment_at(segments: &[Segment], time: f64) -> Option<Segment> {
    let time = (time * 1000.0) as u64;
    segments
        .iter()
        .copied()
        .find(|segment| (segment.start..segment.end).contains(&time))
}

/// Creates the mpv core and makes it wake the main loop for its events.
fn create_mpv() -> Result<Mpv, libmpv2::Error> {
    let mut mpv = Mpv::with_initializer(|init| {
        init.set_property("vo", "libmpv")?;
        init.set_property("keep-open", "yes")?;
        init.set_property("cache", "yes")?;
        // Decode on the GPU where that is known to work, else in software.
        init.set_property("hwdec", "auto-safe")?;
        // Give a server that went quiet half a minute, then try again.
        init.set_property("network-timeout", 30i64)?;
        init.set_property("stream-lavf-o", RECONNECT)?;
        init.set_property("demuxer-lavf-o", RECONNECT)?;
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
    #[test]
    fn an_item_counts_from_where_it_starts_in_its_file() {
        assert_eq!(item_time(12.5, 0.0), 12.5);
        assert_eq!(item_time(192.5, 180.0), 12.5);
        // mpv may report a moment before the start while it seeks there.
        assert_eq!(item_time(179.9, 180.0), 0.0);
    }

    #[test]
    fn a_position_is_in_the_intro_the_credits_or_neither() {
        let segments = [
            Segment {
                kind: SegmentKind::Intro,
                start: 5_000,
                end: 35_000,
            },
            Segment {
                kind: SegmentKind::Outro,
                start: 270_000,
                end: 300_000,
            },
        ];
        let kind = |time| segment_at(&segments, time).map(|segment| segment.kind);
        assert_eq!(kind(4.9), None);
        assert_eq!(kind(5.0), Some(SegmentKind::Intro));
        assert_eq!(kind(34.9), Some(SegmentKind::Intro));
        // The end is where a skip lands: no longer in it.
        assert_eq!(kind(35.0), None);
        assert_eq!(kind(280.0), Some(SegmentKind::Outro));
        assert_eq!(segment_at(&[], 10.0), None);
    }

}
