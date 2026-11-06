//! Playback as far as the window is concerned: starting it, the resume
//! question, fullscreen and keeping the screen on.

use super::*;

impl PlexWindow {
    pub(super) fn setup_player(&self) {
        let imp = self.imp();
        let player = Player::new();

        imp._player_view.set_player(&player);
        let music_controls = MediaControls::new(&player, false);
        music_controls.add_music(&imp._media_box_music);
        let _ = imp.music_controls.set(music_controls);

        player.connect(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |event| window.on_player_event(event)
        ));

        imp._section_view.connect_play(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |section, sort, shuffle| {
                window.play(
                    PlayTarget::Section { section, sort },
                    shuffle,
                    StartMode::Ask,
                )
            }
        ));

        imp._player_view.connect_fullscreen(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |fullscreen| {
                if fullscreen {
                    window.fullscreen();
                } else {
                    window.unfullscreen();
                }
            }
        ));
        imp._player_view.connect_windowed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |windowed| window.set_chrome_visible(!windowed)
        ));
        self.connect_fullscreened_notify(|window| window.on_fullscreen_changed());

        crate::mpris::start(&player, self.upcast_ref());

        let _ = imp.player.set(player);
    }

    pub(super) fn play(&self, target: PlayTarget, shuffle: bool, mode: StartMode) {
        let imp = self.imp();
        let Some(player) = imp.player.get() else {
            return;
        };
        if imp.session.server().is_some() {
            player.play(target, shuffle, mode);
            return;
        }
        // Without a connection only offline copies can play: the chosen one
        // and what follows it in the list.
        let (PlayTarget::Item(key), Some(machine), Some(user)) = (
            target,
            imp.session.machine_identifier(),
            imp.session.user_key(),
        ) else {
            return;
        };
        let items = sync::offline_items(&machine, &user);
        match items.iter().position(|item| item.rating_key == key) {
            Some(index) => player.play_local(items[index..].to_vec()),
            None => crate::notify::message(&gettext(
                "This is not available without a connection to the server",
            )),
        }
    }

    pub(super) fn on_player_event(&self, event: &Event) {
        let imp = self.imp();
        match event {
            Event::Started { video, .. } => {
                imp.video_playing.set(*video);
                imp.video_paused.set(false);
                self.update_idle_inhibit();
                imp._sidebar_box.set_player_visible(*video);
                if *video {
                    self.open(Location::Player);
                }
            }
            Event::AskResume(item) => self.ask_resume(&item.title),
            Event::Paused(paused) => {
                imp.video_paused.set(*paused);
                self.update_idle_inhibit();
            }
            Event::Stopped => {
                imp.video_playing.set(false);
                self.update_idle_inhibit();
                imp._sidebar_box.set_player_visible(false);
                self.leave_player();
            }
            _ => {}
        }
    }

    /// Asks whether to resume a partly watched item or start over.
    pub(super) fn ask_resume(&self, title: &str) {
        let dialog = adw::AlertDialog::new(
            Some(title),
            Some(&gettext(
                "Do you want to resume watching or start from the beginning?",
            )),
        );
        dialog.add_response("_beginning_button", &gettext("Start from the beginning"));
        dialog.add_response("_resume_button", &gettext("Resume"));
        dialog.set_response_appearance("_resume_button", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("_resume_button"));
        dialog.connect_response(
            None,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_, response| {
                    let mode = match response {
                        "_resume_button" => StartMode::Resume,
                        "_beginning_button" => StartMode::Beginning,
                        _ => return,
                    };
                    if let Some(player) = window.imp().player.get() {
                        player.start_current(mode);
                    }
                }
            ),
        );
        dialog.present(Some(self));
    }

    /// Hides or shows everything around the video.
    pub(super) fn set_chrome_visible(&self, visible: bool) {
        let imp = self.imp();
        imp.content_header.set_visible(visible);
        imp._split_view.set_pin_sidebar(!visible);
        imp._split_view.set_show_sidebar(visible);
    }

    pub(super) fn on_fullscreen_changed(&self) {
        let imp = self.imp();
        let fullscreen = self.is_fullscreen();
        imp._player_view.set_fullscreen_state(fullscreen);
        self.set_chrome_visible(!fullscreen);
    }

    /// Keeps the screen on while a video is playing, fullscreen or not.
    pub(super) fn update_idle_inhibit(&self) {
        let imp = self.imp();
        let Some(application) = self.application() else {
            return;
        };
        let wanted = imp.video_playing.get() && !imp.video_paused.get();
        if wanted {
            if imp.inhibit_cookie.get().is_none() {
                let cookie = application.inhibit(
                    Some(self),
                    gtk::ApplicationInhibitFlags::IDLE,
                    Some("Girens is playing a video"),
                );
                imp.inhibit_cookie.set(Some(cookie));
            }
        } else if let Some(cookie) = imp.inhibit_cookie.take() {
            application.uninhibit(cookie);
        }
    }
}
