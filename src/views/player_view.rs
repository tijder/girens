//! The video page: the picture with its overlay controls, details of what is
//! playing, audio and subtitle choice and what is on deck in the library.

use girens_model::ItemId;
use std::cell::{Cell, OnceCell, RefCell};
use std::time::Instant;

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_model::model::{Item, ItemKind, SegmentKind, StreamKind};
use gtk::{gdk, glib, CompositeTemplate};

use super::{List, MediaBoxVideoBottom, MediaBoxVideoTop, MediaControls};
use crate::i18n::{gettext, gettext_f};
use crate::player::render::VideoRenderer;
use crate::player::{Event, Player, SubtitlePictures, CONTROLS_HIDE_DELAY};
use crate::session::Session;
use crate::{runtime, util};

/// Two clicks within this time toggle fullscreen.
const DOUBLE_CLICK: f64 = 0.4;
/// Seconds from the start of the credits until the next episode starts.
const NEXT_COUNTDOWN: u32 = 15;
/// The speeds offered, as the speed menu lists them.
const SPEEDS: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

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
        pub _skip_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _scroll_controller: TemplateChild<gtk::EventControllerScroll>,
        #[template_child]
        pub _next_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub _next_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _next_now_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _next_cancel_button: TemplateChild<gtk::Button>,
        /// Counts down to the next episode during the credits.
        pub next_countdown: RefCell<Option<glib::SourceId>>,
        pub next_seconds: Cell<u32>,
        /// The user chose to watch the credits of this item.
        pub next_cancelled: Cell<bool>,
        #[template_child]
        pub _speed_box: TemplateChild<adw::ComboRow>,
        pub updating_speed: Cell<bool>,
        /// The title of what plays, for the header bar.
        pub title: gtk::Label,
        #[template_child]
        pub _subtitle_picture: TemplateChild<gtk::Picture>,
        /// The sheets subtitle pictures are cut out of, by where they
        /// come from.
        pub subtitle_sheets: RefCell<std::collections::HashMap<String, gdk::Texture>>,
        /// The subtitles that should be on screen, also while their sheet
        /// is still on its way.
        pub subtitles: RefCell<Option<SubtitlePictures>>,
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
        pub _details_list: TemplateChild<gtk::ListBox>,
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
        pub _version_box: TemplateChild<adw::ComboRow>,
        /// The files of the item that is loaded, as the menu lists them.
        pub versions: RefCell<Vec<ItemId>>,
        #[template_child]
        pub _options_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub _deck_shows_box: TemplateChild<List>,

        pub player: OnceCell<Player>,
        pub controls: OnceCell<MediaControls>,
        pub renderer: OnceCell<VideoRenderer>,
        /// Plex stream ids behind the rows of the two combo rows.
        pub audio_streams: RefCell<Vec<ItemId>>,
        pub subtitle_streams: RefCell<Vec<Option<ItemId>>>,
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

/// Changes the stored volume (0–100), which the player follows.
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
        self.setup_tracks_popover();

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
            move |_, key, _, state| view.handle_key(key, state)
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
        imp._skip_button.connect_clicked(glib::clone!(
            #[strong]
            player,
            move |_| player.skip_segment()
        ));
        imp._next_now_button.connect_clicked(glib::clone!(
            #[strong]
            player,
            move |_| player.next()
        ));
        imp._next_cancel_button.connect_clicked(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                view.imp().next_cancelled.set(true);
                view.stop_next_countdown();
            }
        ));
        // The wheel over the video turns the volume.
        imp._scroll_controller.connect_scroll(glib::clone!(
            #[strong]
            player,
            move |_, _, dy| {
                if !player.is_playing() {
                    return glib::Propagation::Proceed;
                }
                let step = if dy < 0.0 { 5 } else { -5 };
                player.set_volume(player.volume() + step);
                glib::Propagation::Stop
            }
        ));
        let speeds: Vec<String> = SPEEDS.iter().map(|speed| format!("{speed}×")).collect();
        let speeds: Vec<&str> = speeds.iter().map(String::as_str).collect();
        imp._speed_box.set_model(Some(&gtk::StringList::new(&speeds)));
        imp._speed_box.set_selected(2);
        imp._speed_box.connect_selected_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |row| {
                let imp = view.imp();
                if imp.updating_speed.get() {
                    return;
                }
                if let (Some(speed), Some(player)) =
                    (SPEEDS.get(row.selected() as usize), imp.player.get())
                {
                    player.set_speed(*speed);
                }
            }
        ));
        imp._right_click.connect_released(glib::clone!(
            #[strong]
            player,
            move |_, _, _, _| player.play_pause()
        ));

        imp._version_box.connect_selected_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |row| {
                let imp = view.imp();
                if imp.updating_streams.get() {
                    return;
                }
                let version = imp.versions.borrow().get(row.selected() as usize).copied();
                if let (Some(version), Some(player)) = (version, imp.player.get()) {
                    player.set_version(version);
                }
            }
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
        imp._media_bottom.set_narrow(width < 460);
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

    /// Audio and subtitle choice in the control bar, so that it can be
    /// reached in fullscreen too. It mirrors the two rows below the video.
    fn setup_tracks_popover(&self) {
        let imp = self.imp();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        for (title, row) in [
            (gettext("Audio"), &imp._audio_box),
            (gettext("Subtitle"), &imp._subtitle_box),
            (gettext("Speed"), &imp._speed_box),
        ] {
            let label = gtk::Label::builder().label(title).xalign(0.0).build();
            label.add_css_class("caption-heading");
            let choice = gtk::DropDown::builder().build();
            row.bind_property("model", &choice, "model")
                .sync_create()
                .build();
            row.bind_property("selected", &choice, "selected")
                .bidirectional()
                .sync_create()
                .build();
            for widget in [label.upcast_ref::<gtk::Widget>(), choice.upcast_ref()] {
                row.bind_property("visible", widget, "visible")
                    .sync_create()
                    .build();
                content.append(widget);
            }
        }
        let popover = gtk::Popover::builder().child(&content).build();
        let button = imp._media_bottom.tracks_button();
        button.set_popover(Some(&popover));
        imp._options_list
            .bind_property("visible", &button, "visible")
            .sync_create()
            .build();
        // Keep the controls up while the choice is open.
        popover.connect_show(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.stop_hide_timeout()
        ));
        popover.connect_closed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                view.stop_hide_timeout();
                view.start_hide_timeout();
            }
        ));
    }

    fn is_playing_video(&self) -> bool {
        self.imp()
            .controls
            .get()
            .is_some_and(MediaControls::is_active)
    }

    /// Offers to skip the intro, or counts down to the next item during
    /// the credits when there is one.
    fn offer_skip(&self, segment: Option<SegmentKind>) {
        let imp = self.imp();
        let has_next = imp.player.get().is_some_and(Player::has_next);
        let intro = segment == Some(SegmentKind::Intro);
        if intro {
            imp._skip_button.set_label(&gettext("Skip intro"));
        }
        imp._skip_button.set_visible(intro);
        if segment == Some(SegmentKind::Outro) && has_next && !imp.next_cancelled.get() {
            self.start_next_countdown();
        } else {
            self.stop_next_countdown();
        }
    }

    /// The title of the item that follows in the queue.
    fn next_title(&self) -> String {
        let Some(player) = self.imp().player.get() else {
            return String::new();
        };
        player
            .queue_items()
            .get(player.queue_index() + 1)
            .map(|item| match (item.kind, item.season_episode()) {
                (ItemKind::Episode, Some(number)) => format!("{number} - {}", item.title),
                _ => item.title.clone(),
            })
            .unwrap_or_default()
    }

    fn start_next_countdown(&self) {
        let imp = self.imp();
        if imp.next_countdown.borrow().is_some() {
            return;
        }
        imp.next_seconds.set(NEXT_COUNTDOWN);
        self.show_next_countdown();
        imp._next_box.set_visible(true);
        let tick = glib::timeout_add_seconds_local(
            1,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    let imp = view.imp();
                    let left = imp.next_seconds.get().saturating_sub(1);
                    imp.next_seconds.set(left);
                    if left == 0 {
                        imp.next_countdown.take();
                        imp._next_box.set_visible(false);
                        if let Some(player) = imp.player.get() {
                            player.next();
                        }
                        return glib::ControlFlow::Break;
                    }
                    view.show_next_countdown();
                    glib::ControlFlow::Continue
                }
            ),
        );
        imp.next_countdown.replace(Some(tick));
    }

    fn show_next_countdown(&self) {
        let imp = self.imp();
        let seconds = imp.next_seconds.get().to_string();
        let title = self.next_title();
        imp._next_label.set_text(&gettext_f(
            "Next in {0} s: {1}",
            &[&seconds, &title],
        ));
    }

    fn stop_next_countdown(&self) {
        let imp = self.imp();
        if let Some(tick) = imp.next_countdown.take() {
            tick.remove();
        }
        imp._next_box.set_visible(false);
    }

    /// The title of what plays, for the header bar to follow.
    pub fn title_label(&self) -> gtk::Label {
        self.imp().title.clone()
    }

    /// Lists the chapters of `item` behind the chapters button, each
    /// jumping to its start.
    fn show_chapters(&self, item: &Item) {
        let imp = self.imp();
        let button = imp._media_bottom.chapters_button();
        button.set_visible(!item.chapters.is_empty());
        if item.chapters.is_empty() {
            button.set_popover(gtk::Popover::NONE);
            return;
        }
        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        for chapter in &item.chapters {
            let title = chapter
                .tag
                .clone()
                .filter(|tag| !tag.is_empty())
                .unwrap_or_else(|| gettext_f("Chapter {}", &[&chapter.index.to_string()]));
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&title))
                .subtitle(util::format_duration(chapter.start_time_offset))
                .activatable(true)
                .build();
            let start = chapter.start_time_offset;
            row.connect_activated(glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |row| {
                    if let Some(player) = view.imp().player.get() {
                        player.seek_to(start);
                    }
                    if let Some(popover) = row.ancestor(gtk::Popover::static_type()) {
                        popover.downcast::<gtk::Popover>().unwrap().popdown();
                    }
                }
            ));
            list.append(&row);
        }
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&list)
            .propagate_natural_height(true)
            .max_content_height(400)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let popover = gtk::Popover::builder().child(&scrolled).build();
        popover.connect_show(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.stop_hide_timeout()
        ));
        popover.connect_closed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                view.stop_hide_timeout();
                view.start_hide_timeout();
            }
        ));
        button.set_popover(Some(&popover));
    }

    /// Draws subtitles that are pictures over the video, fetching the
    /// sheets they are cut out of when those are not there yet.
    fn show_subtitles(&self, subtitles: Option<SubtitlePictures>) {
        let imp = self.imp();
        imp.subtitles.replace(subtitles.clone());
        let Some(subtitles) = subtitles else {
            imp._subtitle_picture.set_paintable(gdk::Paintable::NONE);
            return;
        };
        let missing: Vec<String> = subtitles
            .cues
            .iter()
            .map(|(sheet, _)| sheet.clone())
            .filter(|sheet| !imp.subtitle_sheets.borrow().contains_key(sheet))
            .collect();
        if missing.is_empty() {
            self.draw_subtitles(&subtitles);
            return;
        }
        // Nothing rather than the previous line while the sheet loads.
        imp._subtitle_picture.set_paintable(gdk::Paintable::NONE);
        let Some(server) = Session::current().server() else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                for sheet in missing {
                    let url = server.download_url(&sheet);
                    let bytes =
                        runtime::run(async move { girens_model::net::download(&url).await }).await;
                    let texture = bytes
                        .map_err(|err| err.to_string())
                        .and_then(|bytes| {
                            gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes))
                                .map_err(|err| err.to_string())
                        });
                    match texture {
                        Ok(texture) => {
                            view.imp().subtitle_sheets.borrow_mut().insert(sheet, texture);
                        }
                        Err(err) => {
                            crate::notify::failure(
                                &gettext("Could not load the subtitles"),
                                &err,
                            );
                            return;
                        }
                    }
                }
                // Unless other subtitles are due by now.
                let current = view.imp().subtitles.borrow().clone();
                if current.as_ref() == Some(&subtitles) {
                    view.draw_subtitles(&subtitles);
                }
            }
        ));
    }

    fn draw_subtitles(&self, subtitles: &SubtitlePictures) {
        let imp = self.imp();
        let sheets = imp.subtitle_sheets.borrow();
        // A frame as wide as the video is shown, so that the picture fits
        // over the video however the window is sized.
        let (canvas_width, height) = subtitles.canvas;
        let width = (f64::from(height) * subtitles.aspect) as f32;
        let stretch = width / canvas_width.max(1.0);
        let snapshot = gtk::Snapshot::new();
        for (sheet, cue) in &subtitles.cues {
            let Some(texture) = sheets.get(sheet) else {
                continue;
            };
            let place = gtk::graphene::Rect::new(
                cue.x * stretch,
                cue.y,
                cue.width * stretch,
                cue.height,
            );
            // The whole sheet, moved so that the part for this cue lands in
            // its place, with the rest cut off.
            let sheet_place = gtk::graphene::Rect::new(
                place.x() - cue.sheet_x * stretch,
                place.y() - cue.sheet_y,
                texture.width() as f32 * stretch,
                texture.height() as f32,
            );
            snapshot.push_clip(&place);
            snapshot.append_texture(texture, &sheet_place);
            snapshot.pop();
        }
        let size = gtk::graphene::Size::new(width, height);
        imp._subtitle_picture
            .set_paintable(snapshot.to_paintable(Some(&size)).as_ref());
    }

    // Player events

    fn on_player_event(&self, event: &Event) {
        match event {
            Event::Started { item, video } => {
                let imp = self.imp();
                imp.next_cancelled.set(false);
                self.stop_next_countdown();
                imp.title.set_label(&match item.kind {
                    ItemKind::Episode => item.grandparent_title.clone().unwrap_or_default(),
                    _ => item.title.clone(),
                });
                if *video {
                    if let Some(item) = imp.player.get().and_then(Player::loaded_item) {
                        self.show_item(&item);
                    }
                    self.show_controls();
                }
            }
            Event::SpeedChanged(speed) => {
                let imp = self.imp();
                let index = SPEEDS
                    .iter()
                    .position(|known| (known - speed).abs() < 0.01)
                    .map_or(gtk::INVALID_LIST_POSITION, |index| index as u32);
                imp.updating_speed.set(true);
                imp._speed_box.set_selected(index);
                imp.updating_speed.set(false);
            }
            Event::Segment(segment) => self.offer_skip(*segment),
            // Also when it was chosen elsewhere than in the menus here.
            Event::StreamsChanged => {
                if let Some(item) = self.imp().player.get().and_then(Player::loaded_item) {
                    self.show_streams(&item);
                }
            }
            Event::Subtitles(subtitles) => self.show_subtitles(subtitles.clone()),
            Event::Stopped => {
                self.offer_skip(None);
                self.stop_next_countdown();
                self.show_subtitles(None);
                self.imp().subtitle_sheets.borrow_mut().clear();
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
        imp._details_list.set_visible(true);
        let set = |row: &adw::ActionRow, text: &str| {
            row.set_subtitle(text);
            row.set_visible(!text.is_empty());
        };
        let title = match (item.kind, &item.grandparent_title, item.season_episode()) {
            (ItemKind::Episode, Some(show), Some(number)) => {
                format!("{show} · {number} · {}", item.title)
            }
            _ => item.title.clone(),
        };
        set(&imp._title_label, &title);
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
        self.show_chapters(item);
        self.show_deck(item);
    }

    fn show_streams(&self, item: &Item) {
        let imp = self.imp();
        let title = |stream: &girens_model::model::Stream| {
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
        // The files an item comes in, such as two resolutions; the one that
        // plays is the first.
        let versions: Vec<String> = item
            .media
            .iter()
            .enumerate()
            .map(|(index, media)| {
                let named = [
                    media.video_resolution.as_ref().map(|height| format!("{height}p")),
                    media.container.as_ref().map(|container| container.to_uppercase()),
                ];
                let name = util::detail_line(named);
                if name.is_empty() {
                    (index + 1).to_string()
                } else {
                    name
                }
            })
            .collect();
        imp._version_box.set_model(Some(&strings(&versions)));
        imp._version_box.set_selected(0);
        imp._version_box.set_visible(versions.len() > 1);
        imp.versions
            .replace(item.media.iter().map(|media| media.id).collect());
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
            .set_visible(audio.len() > 1 || !subtitles.is_empty() || versions.len() > 1);
    }

    /// What is on deck in the library of the playing item.
    /// Below an episode the rest of its season; below anything else what
    /// is left to watch in its library.
    fn show_deck(&self, item: &Item) {
        let imp = self.imp();
        imp._deck_shows_box.clear();
        let season = match item.kind {
            ItemKind::Episode => item.parent_rating_key,
            _ => None,
        };
        let section = match item.kind {
            ItemKind::Clip | ItemKind::Track => None,
            _ => item.library_section_id,
        };
        imp._label.set_label(&if season.is_some() {
            gettext("More of this season")
        } else {
            gettext("Continue watching")
        });
        imp._label.set_visible(season.is_some() || section.is_some());
        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let items = runtime::run(async move {
                    match (season, section) {
                        (Some(season), _) => server.children(season).await,
                        (None, Some(section)) => server.section_on_deck(section).await,
                        (None, None) => Ok(Vec::new()),
                    }
                })
                .await;
                match items {
                    Ok(items) => view.imp()._deck_shows_box.set_items(items),
                    Err(err) => crate::notify::failure(&crate::messages::load_on_deck(), &err),
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
    pub fn handle_key(&self, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        let Some(player) = self.imp().player.get() else {
            return glib::Propagation::Proceed;
        };
        if !player.is_playing() {
            return glib::Propagation::Proceed;
        }
        if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
            return glib::Propagation::Proceed;
        }
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let video = self.is_playing_video();
        match key {
            gdk::Key::f | gdk::Key::F11 if video => self.toggle_fullscreen(),
            gdk::Key::t if video => self.toggle_windowed(),
            gdk::Key::space | gdk::Key::p | gdk::Key::k => player.play_pause(),
            gdk::Key::o => self.show_controls(),
            gdk::Key::q => player.stop(),
            gdk::Key::comma => player.previous(),
            gdk::Key::period => player.next(),
            gdk::Key::s => player.skip_segment(),
            gdk::Key::Left => player.seek_by(if shift { -60 } else { -10 }),
            gdk::Key::Right => player.seek_by(if shift { 60 } else { 10 }),
            gdk::Key::bracketleft => player.seek_by(-30),
            gdk::Key::bracketright => player.seek_by(30),
            gdk::Key::Page_Up => player.seek_chapter(-1),
            gdk::Key::Page_Down => player.seek_chapter(1),
            gdk::Key::Up => player.set_volume(player.volume() + 5),
            gdk::Key::Down => player.set_volume(player.volume() - 5),
            gdk::Key::m => player.set_muted(!player.is_muted()),
            gdk::Key::less | gdk::Key::greater => {
                let step = if key == gdk::Key::less { -0.25 } else { 0.25 };
                player.set_speed(player.speed() + step);
                crate::notify::message(&gettext_f("Speed {}×", &[&player.speed().to_string()]));
            }
            gdk::Key::v => {
                let settings = gtk::gio::Settings::new(crate::config::APP_ID);
                let prefer = !settings.boolean("prefer-music-clips");
                if let Err(err) = settings.set_boolean("prefer-music-clips", prefer) {
                    eprintln!("Could not save the music clip preference: {err}");
                }
            }
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
