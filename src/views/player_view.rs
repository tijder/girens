//! The video page: the picture with its overlay controls, details of what is
//! playing, audio and subtitle choice and what is on deck in the library.

use std::cell::{Cell, OnceCell, RefCell};
use std::time::Instant;

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_plex::model::{Item, ItemKind, StreamKind};
use gtk::{gdk, glib, CompositeTemplate};

use super::{List, MediaBoxVideoBottom, MediaBoxVideoTop, MediaControls};
use crate::i18n::gettext;
use crate::player::render::VideoRenderer;
use crate::player::{Event, Player, CONTROLS_HIDE_DELAY};
use crate::session::Session;
use crate::{runtime, util};

/// Two clicks within this time toggle fullscreen.
const DOUBLE_CLICK: f64 = 0.4;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/player_view.ui")]
    pub struct PlayerView {
        #[template_child]
        pub _frame: TemplateChild<gtk::GLArea>,
        #[template_child]
        pub _key_controller: TemplateChild<gtk::EventControllerKey>,
        #[template_child]
        pub _motion_controller: TemplateChild<gtk::EventControllerMotion>,
        #[template_child]
        pub _click: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub _right_click: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub _controlls_top: TemplateChild<gtk::Box>,
        #[template_child]
        pub _media_top: TemplateChild<MediaBoxVideoTop>,
        #[template_child]
        pub _media_bottom: TemplateChild<MediaBoxVideoBottom>,
        #[template_child]
        pub _video_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub _card_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub _revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub _label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _title_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _release_datum_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _genre_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _duration_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _discription_label: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _subtitle_box: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub _audio_box: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub _options_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub _deck_shows_box: TemplateChild<List>,

        pub player: OnceCell<Player>,
        pub controls: OnceCell<MediaControls>,
        pub renderer: OnceCell<VideoRenderer>,
        /// Plex stream ids behind the rows of the two combo rows.
        pub audio_streams: RefCell<Vec<u64>>,
        pub subtitle_streams: RefCell<Vec<Option<u64>>>,
        pub updating_streams: Cell<bool>,
        pub fullscreen: Cell<bool>,
        pub windowed: Cell<bool>,
        /// Height of the video when it does not fill the window.
        pub video_height: Cell<i32>,
        pub hide_timeout: RefCell<Option<glib::SourceId>>,
        pub last_click: Cell<Option<Instant>>,
        pub last_pointer: Cell<(f64, f64)>,
        pub deck_task: RefCell<Option<glib::JoinHandle<()>>>,
        pub on_fullscreen: RefCell<Option<Box<dyn Fn(bool)>>>,
        pub on_windowed: RefCell<Option<Box<dyn Fn(bool)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlayerView {
        const NAME: &'static str = "player_view";
        type Type = super::PlayerView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            List::ensure_type();
            MediaBoxVideoTop::ensure_type();
            MediaBoxVideoBottom::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlayerView {}
    impl WidgetImpl for PlayerView {}
    impl BinImpl for PlayerView {}
}

glib::wrapper! {
    pub struct PlayerView(ObjectSubclass<imp::PlayerView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// `1 hr 42 min`, as shown in the details.
fn duration_text(milliseconds: u64) -> String {
    let minutes = milliseconds / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}

impl PlayerView {
    /// Connects the page to the player. Called once by the window.
    pub fn set_player(&self, player: &Player) {
        let imp = self.imp();
        if imp.player.set(player.clone()).is_err() {
            return;
        }
        player.connect_mpv_created(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |mpv| {
                let imp = view.imp();
                // The surface has to exist before mpv opens a video.
                imp._frame.realize();
                let _ = imp.renderer.set(VideoRenderer::attach(&imp._frame, mpv));
            }
        ));

        let controls = MediaControls::new(player, true);
        controls.add_video_top(&imp._media_top);
        controls.add_video_bottom(&imp._media_bottom);
        controls.connect_fullscreen(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move || view.toggle_fullscreen()
        ));
        controls.connect_windowed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move || view.toggle_windowed()
        ));
        // Keep the controls up while the play queue is open.
        controls.connect_popover(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |open| {
                view.stop_hide_timeout();
                if !open {
                    view.start_hide_timeout();
                }
            }
        ));
        let _ = imp.controls.set(controls);

        player.connect(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |event| view.on_player_event(event)
        ));

        imp._key_controller.connect_key_pressed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| view.handle_key(key)
        ));
        imp._motion_controller.connect_motion(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, x, y| {
                // Some compositors repeat motion events for a resting pointer.
                if view.imp().last_pointer.replace((x, y)) != (x, y) {
                    view.show_controls();
                }
            }
        ));
        imp._click.connect_pressed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                let now = Instant::now();
                let double = view
                    .imp()
                    .last_click
                    .replace(Some(now))
                    .is_some_and(|last| now.duration_since(last).as_secs_f64() < DOUBLE_CLICK);
                if double {
                    view.imp().last_click.set(None);
                    view.toggle_fullscreen();
                } else {
                    view.show_controls();
                }
            }
        ));
        imp._right_click.connect_released(glib::clone!(
            #[strong]
            player,
            move |_, _, _, _| player.play_pause()
        ));

        imp._audio_box.connect_selected_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |row| {
                let imp = view.imp();
                if imp.updating_streams.get() {
                    return;
                }
                let stream = imp
                    .audio_streams
                    .borrow()
                    .get(row.selected() as usize)
                    .copied();
                if let (Some(stream), Some(player)) = (stream, imp.player.get()) {
                    player.set_audio_stream(stream);
                }
            }
        ));
        imp._subtitle_box.connect_selected_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |row| {
                let imp = view.imp();
                if imp.updating_streams.get() {
                    return;
                }
                let stream = imp
                    .subtitle_streams
                    .borrow()
                    .get(row.selected() as usize)
                    .copied();
                if let (Some(stream), Some(player)) = (stream, imp.player.get()) {
                    player.set_subtitle_stream(stream);
                }
            }
        ));
    }

    pub fn connect_fullscreen(&self, callback: impl Fn(bool) + 'static) {
        self.imp().on_fullscreen.replace(Some(Box::new(callback)));
    }

    pub fn connect_windowed(&self, callback: impl Fn(bool) + 'static) {
        self.imp().on_windowed.replace(Some(Box::new(callback)));
    }

    /// Adapts the page to the width of the window.
    pub fn set_window_width(&self, width: i32) {
        let imp = self.imp();
        imp._deck_shows_box
            .set_cover_width(if width < 450 { width / 2 - 10 } else { 200 });
        imp._card_box.set_orientation(if width < 850 {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        let height = match width {
            ..850 => 300,
            850..1500 => 500,
            _ => 800,
        };
        imp.video_height.set(height);
        if !imp.fullscreen.get() && !imp.windowed.get() {
            imp._video_box.set_size_request(-1, height);
        }
    }

    /// Shows the new state of an item in the on-deck row.
    pub fn update_item(&self, item: &Item) {
        self.imp()._deck_shows_box.update_item(item);
    }

    fn is_playing_video(&self) -> bool {
        self.imp()
            .controls
            .get()
            .is_some_and(MediaControls::is_active)
    }

    // Player events

    fn on_player_event(&self, event: &Event) {
        match event {
            Event::Started { video: true, .. } => {
                if let Some(item) = self.imp().player.get().and_then(Player::loaded_item) {
                    self.show_item(&item);
                }
                self.show_controls();
            }
            Event::Stopped => {
                if self.imp().fullscreen.get() {
                    self.toggle_fullscreen();
                } else if self.imp().windowed.get() {
                    self.toggle_windowed();
                }
            }
            _ => {}
        }
    }

    fn show_item(&self, item: &Item) {
        let imp = self.imp();
        let set = |row: &adw::ActionRow, text: &str| {
            row.set_title(text);
            row.set_visible(!text.is_empty());
        };
        set(&imp._title_label, &item.title);
        set(
            &imp._release_datum_label,
            &item
                .originally_available_at
                .as_deref()
                .and_then(util::format_date)
                .unwrap_or_default(),
        );
        let genres: Vec<&str> = item.genres.iter().map(|genre| genre.tag.as_str()).collect();
        set(&imp._genre_label, &genres.join(" "));
        set(
            &imp._duration_label,
            &item.duration.map(duration_text).unwrap_or_default(),
        );
        set(
            &imp._discription_label,
            item.summary.as_deref().unwrap_or_default(),
        );

        self.show_streams(item);
        self.show_deck(item);
    }

    fn show_streams(&self, item: &Item) {
        let imp = self.imp();
        let title = |stream: &girens_plex::model::Stream| {
            stream
                .display_title
                .clone()
                .or_else(|| stream.title.clone())
                .unwrap_or_else(|| stream.id.to_string())
        };
        // Like the server, only the first file of an item is considered.
        let streams = |kind| {
            item.parts()
                .next()
                .into_iter()
                .flat_map(|part| part.streams.iter())
                .filter(move |stream| stream.kind() == kind)
        };

        let audio: Vec<_> = streams(StreamKind::Audio).collect();
        let subtitles: Vec<_> = streams(StreamKind::Subtitle).collect();
        let audio_titles: Vec<String> = audio.iter().map(|stream| title(stream)).collect();
        let mut subtitle_titles = vec![gettext("None")];
        subtitle_titles.extend(subtitles.iter().map(|stream| title(stream)));

        imp.updating_streams.set(true);
        let strings = |titles: &[String]| {
            gtk::StringList::new(&titles.iter().map(String::as_str).collect::<Vec<_>>())
        };
        imp._audio_box.set_model(Some(&strings(&audio_titles)));
        imp._audio_box.set_selected(
            audio
                .iter()
                .position(|stream| stream.selected)
                .map_or(gtk::INVALID_LIST_POSITION, |index| index as u32),
        );
        imp._subtitle_box
            .set_model(Some(&strings(&subtitle_titles)));
        imp._subtitle_box.set_selected(
            subtitles
                .iter()
                .position(|stream| stream.selected)
                .map_or(0, |index| index as u32 + 1),
        );
        imp.updating_streams.set(false);

        imp.audio_streams
            .replace(audio.iter().map(|stream| stream.id).collect());
        let mut subtitle_ids = vec![None];
        subtitle_ids.extend(subtitles.iter().map(|stream| Some(stream.id)));
        imp.subtitle_streams.replace(subtitle_ids);

        imp._audio_box.set_visible(audio.len() > 1);
        imp._subtitle_box.set_visible(!subtitles.is_empty());
        imp._options_list
            .set_visible(audio.len() > 1 || !subtitles.is_empty());
    }

    /// What is on deck in the library of the playing item.
    fn show_deck(&self, item: &Item) {
        let imp = self.imp();
        imp._deck_shows_box.clear();
        let section = match item.kind {
            ItemKind::Clip | ItemKind::Track => None,
            _ => item.library_section_id,
        };
        imp._label.set_visible(section.is_some());
        let (Some(section), Some(server)) = (section, Session::current().server()) else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                match runtime::run(async move { server.section_on_deck(section).await }).await {
                    Ok(items) => view.imp()._deck_shows_box.set_items(items),
                    Err(err) => crate::notify::failure(
                        &crate::i18n::gettext("Could not load on deck"),
                        &err,
                    ),
                }
            }
        ));
        if let Some(previous) = imp.deck_task.replace(Some(task)) {
            previous.abort();
        }
    }

    // Keyboard

    /// Handles a playback shortcut. The window forwards keys that no other
    /// widget used, so the shortcuts work on every page.
    pub fn handle_key(&self, key: gdk::Key) -> glib::Propagation {
        let Some(player) = self.imp().player.get() else {
            return glib::Propagation::Proceed;
        };
        if !player.is_playing() {
            return glib::Propagation::Proceed;
        }
        let video = self.is_playing_video();
        match key {
            gdk::Key::f | gdk::Key::F11 if video => self.toggle_fullscreen(),
            gdk::Key::t if video => self.toggle_windowed(),
            gdk::Key::space | gdk::Key::p | gdk::Key::k => player.play_pause(),
            gdk::Key::o => self.show_controls(),
            gdk::Key::q => player.stop(),
            gdk::Key::comma => player.previous(),
            gdk::Key::period => player.next(),
            gdk::Key::bracketleft | gdk::Key::Left => player.seek_by(-10),
            gdk::Key::bracketright | gdk::Key::Right => player.seek_by(30),
            gdk::Key::Escape if self.imp().fullscreen.get() => self.toggle_fullscreen(),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    // Overlay controls and cursor

    fn stop_hide_timeout(&self) {
        if let Some(timeout) = self.imp().hide_timeout.take() {
            timeout.remove();
        }
    }

    fn start_hide_timeout(&self) {
        let timeout = glib::timeout_add_local_once(
            CONTROLS_HIDE_DELAY,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || {
                    let imp = view.imp();
                    imp.hide_timeout.take();
                    if let Some(controls) = imp.controls.get() {
                        controls.set_revealed(false);
                    }
                    if imp.fullscreen.get() {
                        view.set_cursor_from_name(Some("none"));
                    }
                }
            ),
        );
        self.imp().hide_timeout.replace(Some(timeout));
    }

    /// Shows the overlay controls for a few seconds.
    fn show_controls(&self) {
        if self.is_playing_video() {
            if let Some(controls) = self.imp().controls.get() {
                controls.set_revealed(true);
            }
        }
        self.stop_hide_timeout();
        self.start_hide_timeout();
        self.set_cursor_from_name(Some("default"));
    }

    // Fullscreen and "windowed fullscreen"

    fn toggle_fullscreen(&self) {
        let fullscreen = !self.imp().fullscreen.get();
        if let Some(callback) = self.imp().on_fullscreen.borrow().as_ref() {
            callback(fullscreen);
        }
    }

    /// Fills the window with the video without leaving it.
    fn toggle_windowed(&self) {
        let imp = self.imp();
        let windowed = !imp.windowed.get();
        imp.windowed.set(windowed);
        self.set_video_only(windowed);
        if let Some(callback) = imp.on_windowed.borrow().as_ref() {
            callback(windowed);
        }
    }

    /// Called by the window when it entered or left fullscreen.
    pub fn set_fullscreen_state(&self, fullscreen: bool) {
        let imp = self.imp();
        imp.fullscreen.set(fullscreen);
        if !fullscreen {
            imp.windowed.set(false);
        }
        if let Some(controls) = imp.controls.get() {
            controls.set_windowed_button_visible(!fullscreen);
        }
        self.set_video_only(fullscreen);
    }

    fn set_video_only(&self, video_only: bool) {
        let imp = self.imp();
        imp._revealer.set_reveal_child(!video_only);
        imp._video_box.set_vexpand(video_only);
        if video_only {
            imp._video_box.set_size_request(-1, -1);
            imp._controlls_top.add_css_class("black_background");
            self.show_controls();
        } else {
            let height = match imp.video_height.get() {
                0 => 500,
                height => height,
            };
            imp._video_box.set_size_request(-1, height);
            imp._controlls_top.remove_css_class("black_background");
            self.set_cursor_from_name(Some("default"));
        }
    }
}
