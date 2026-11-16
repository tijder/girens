//! Telling the server what is playing and how far it is, and keeping the
//! conversion it does for the player going.

use girens_model::model::ItemKind;
use girens_model::PlaybackState;
use gtk::glib;

use super::{Inner, Playing};
use crate::runtime;
use crate::session::Session;

impl Inner {
    pub(super) fn report_timeline(&self, playback: PlaybackState) {
        let state = self.state.borrow();
        if let Some(playing) = state.current.as_ref() {
            self.send_timeline(playing, playback);
        }
    }

    pub(super) fn send_timeline(&self, playing: &Playing, playback: PlaybackState) {
        let session_state = Session::current();
        // Progress is reported for the queue entry, with the duration of what
        // is actually playing.
        let mut item = playing.item.clone();
        item.duration = playing.loading.duration.or(item.duration);
        // Servers count from the start of the file.
        let position = ((playing.position + playing.base) * 1000.0) as u64;

        // An offline copy of a video remembers the position itself, for
        // when the server cannot be asked.
        let key = item.rating_key;
        let copy = match (
            matches!(item.kind, ItemKind::Movie | ItemKind::Episode),
            session_state.machine_identifier(),
            session_state.user_key(),
        ) {
            (true, Some(machine), Some(user)) => {
                crate::sync::record_progress(&machine, &user, key, position, item.duration)
                    .map(|offset| (machine, user, offset))
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
                (Ok(()), Some((machine, user, offset))) => {
                    crate::sync::progress_sent(&machine, &user, key, offset);
                }
                (Ok(()), None) => {}
                (Err(err), _) => log::warn!("Could not report the playback position: {err}"),
            }
        });
    }

    /// Tells the server it can stop transcoding for what was playing.
    pub(super) fn end_transcode_session(&self, playing: &Playing) {
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
                log::warn!("Could not end the transcode session: {err}");
            }
        });
    }

    /// Tells a server that would otherwise think the player gone that it
    /// is only paused.
    pub(super) fn report_when_paused(&self) {
        let wanted = Session::current()
            .server()
            .is_some_and(|server| server.wants_reports_when_paused());
        let paused = self
            .state
            .borrow()
            .current
            .as_ref()
            .is_some_and(|playing| playing.started && playing.paused);
        if wanted && paused {
            self.report_timeline(PlaybackState::Paused);
        }
    }

    pub(super) fn ping_session(&self) {
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
                log::warn!("Could not keep the transcode session alive: {err}");
            }
        });
    }
}
