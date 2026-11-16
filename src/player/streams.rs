//! The audio and subtitle streams of what is playing: making mpv use the
//! ones chosen on the server, and drawing subtitles that are pictures.

use std::rc::Rc;

use girens_model::model::StreamKind;
use girens_model::ItemId;
use gtk::glib;

use super::{EndAction, Event, Inner, SubtitlePictures};
use crate::runtime;
use crate::session::Session;

struct MpvTrack {
    kind: String,
    id: i64,
    ff_index: Option<i64>,
    title: Option<String>,
}

impl Inner {
    pub(super) fn on_tracks_available(self: &Rc<Self>) {
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
    pub(super) fn apply_streams(self: &Rc<Self>) {
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
            let id = if server.streams_in_order() {
                // The stream offers the audio of the file one by one.
                loading
                    .streams(StreamKind::Audio)
                    .position(|other| other.id == stream.id)
                    .and_then(|position| {
                        tracks
                            .iter()
                            .filter(|track| track.kind == "audio")
                            .nth(position)
                    })
                    .map(|track| track.id)
            } else {
                let index = stream.index.map(i64::from);
                find("audio", &|track| index.is_some() && track.ff_index == index)
            };
            if let Some(id) = id {
                self.set_mpv("aid", id);
            }
        }

        let subtitle = loading.selected_stream(StreamKind::Subtitle);
        let pictures = subtitle.filter(|stream| stream.pictures);
        self.use_pictures(&server, loading.rating_key, pictures);
        match subtitle {
            None => self.set_mpv("sid", "no"),
            // Drawn by the player itself, over the video.
            Some(stream) if stream.pictures => self.set_mpv("sid", "no"),
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

    /// Fetches the pictures of `stream` to draw them while playing, or
    /// stops drawing any when another kind of subtitles was chosen.
    pub(super) fn use_pictures(
        self: &Rc<Self>,
        server: &crate::backend::Server,
        item: ItemId,
        stream: Option<&girens_model::model::Stream>,
    ) {
        let wanted = stream.and_then(|stream| Some((stream.id, stream.key.clone()?)));
        let had_any = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.current.as_mut() else {
                return;
            };
            let loaded = playing.pictures.as_ref().map(|(id, _)| *id);
            if loaded.is_some() && loaded == wanted.as_ref().map(|(id, _)| *id) {
                return;
            }
            playing.pictures = None;
            !std::mem::take(&mut playing.shown).is_empty()
        };
        if had_any {
            self.emit(Event::Subtitles(None));
        }
        let Some((id, key)) = wanted else {
            return;
        };
        let (inner, server) = (self.clone(), server.clone());
        glib::spawn_future_local(async move {
            let fetched =
                runtime::run(async move { server.picture_subtitles(&key).await }).await;
            let pictures = match fetched {
                Ok(pictures) => Rc::new(pictures),
                Err(err) => {
                    crate::notify::failure(
                        &crate::i18n::gettext("Could not load the subtitles"),
                        &err,
                    );
                    return;
                }
            };
            let mut state = inner.state.borrow_mut();
            // Unless something else plays by now, or other subtitles were
            // chosen.
            let still_wanted = state.current.as_ref().is_some_and(|playing| {
                playing.loading.rating_key == item
                    && playing
                        .loading
                        .selected_stream(StreamKind::Subtitle)
                        .is_some_and(|stream| stream.id == id)
            });
            if let (true, Some(playing)) = (still_wanted, state.current.as_mut()) {
                playing.pictures = Some((id, pictures));
            }
        });
    }

    /// Tells which subtitle pictures are on screen at `time`, in seconds
    /// into the file, when that changed.
    pub(super) fn show_pictures(&self, time: f64) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.current.as_mut() else {
                return;
            };
            let Some((_, pictures)) = playing.pictures.clone() else {
                return;
            };
            let active = pictures.active_at((time.max(0.0) * 1000.0) as u64);
            if active == playing.shown {
                return;
            }
            playing.shown.clone_from(&active);
            (!active.is_empty()).then(|| {
                let cues = active
                    .into_iter()
                    .filter_map(|cue| Some((pictures.sheets.get(cue.sheet)?.clone(), cue)))
                    .collect();
                (pictures, cues)
            })
        };
        let subtitles = changed.map(|(pictures, cues)| {
            let canvas = (pictures.width, pictures.height);
            let aspect = self
                .mpv
                .get()
                .and_then(|mpv| mpv.get_property::<f64>("video-params/aspect").ok())
                .filter(|aspect| *aspect > 0.0)
                .unwrap_or(f64::from(canvas.0) / f64::from(canvas.1.max(1.0)));
            SubtitlePictures {
                canvas,
                aspect,
                cues,
            }
        });
        self.emit(Event::Subtitles(subtitles));
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
    pub(super) fn select_stream(self: Rc<Self>, kind: StreamKind, stream_id: Option<ItemId>) {
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

            let mut updated = updated;
            self.prefer_version(&mut updated);
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
            if transcoding.is_some() {
                self.emit(Event::StreamsChanged);
            }
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
