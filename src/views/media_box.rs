//! The playback controls: the music bar at the bottom of the window and the
//! two overlays on the video. The widgets only hold the template children;
//! [`MediaControls`] connects any number of them to the player, like
//! `MediaBox` did in the Python version.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::subclass::prelude::*;
use girens_model::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{gdk, glib, CompositeTemplate};

use super::{MusicPopoverMenu, QueuePopover};
use crate::player::{Event, Player};
use crate::session::Session;
use crate::{images, util};

/// Declares a template widget that is an `Adw.Bin` around a `Gtk.Revealer`
/// and exposes the listed template children.
macro_rules! media_box_widget {
    ($module:ident, $name:ident, $gtype:literal, $resource:literal, { $($field:ident: $type:ty),* $(,)? }) => {
        mod $module {
            use super::*;

            mod imp {
                use super::*;

                #[derive(Default, CompositeTemplate)]
                #[template(resource = $resource)]
                pub struct $name {
                    #[template_child]
                    pub _revealer: TemplateChild<gtk::Revealer>,
                    $(
                        #[template_child]
                        pub $field: TemplateChild<$type>,
                    )*
                }

                #[glib::object_subclass]
                impl ObjectSubclass for $name {
                    const NAME: &'static str = $gtype;
                    type Type = super::$name;
                    type ParentType = adw::Bin;

                    fn class_init(klass: &mut Self::Class) {
                        klass.bind_template();
                    }

                    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
                        obj.init_template();
                    }
                }

                impl ObjectImpl for $name {}
                impl WidgetImpl for $name {}
                impl BinImpl for $name {}
            }

            glib::wrapper! {
                pub struct $name(ObjectSubclass<imp::$name>)
                    @extends adw::Bin, gtk::Widget,
                    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
            }
        }
        pub use $module::$name;
    };
}

media_box_widget!(music, MediaBoxMusic, "media_box_music", "/nl/g4d/Girens/media_box_music.ui", {
    _scale_adjustment: gtk::Adjustment,
    _time_left_label: gtk::Label,
    _time_right_label: gtk::Label,
    _cover_image: gtk::Image,
    _playqueue_button: gtk::MenuButton,
    _title_label: gtk::Label,
    _subtitle_label: gtk::Label,
    _label_box: gtk::Box,
    _label2_box: gtk::Box,
    _prev_button: gtk::Button,
    _play_button: gtk::Button,
    _play_image: gtk::Image,
    _next_button: gtk::Button,
    _close_button: gtk::Button,
    _volume_button: gtk::ScaleButton,
    _media_settings: gtk::MenuButton,
});

impl MediaBoxMusic {
    /// In a narrow window the title moves to its own line above the buttons.
    pub fn set_narrow(&self, narrow: bool) {
        let imp = self.imp();
        let (from, to) = if narrow {
            (&imp._label_box, &imp._label2_box)
        } else {
            (&imp._label2_box, &imp._label_box)
        };
        for label in [&imp._title_label, &imp._subtitle_label] {
            let moved = label
                .parent()
                .is_some_and(|parent| parent == *from.upcast_ref::<gtk::Widget>());
            if moved {
                from.remove(&**label);
                to.append(&**label);
            }
        }
        // Without the cover the buttons fit in the narrowest window.
        if let Some(cover) = imp._cover_image.parent() {
            cover.set_visible(!narrow);
        }
    }
}

media_box_widget!(video_top, MediaBoxVideoTop, "media_box_video_top", "/nl/g4d/Girens/media_box_video_top.ui", {
    _fullscreen_button: gtk::Button,
    _fullscreen_windowed_button: gtk::Button,
    _title_label: gtk::Label,
    _subtitle_label: gtk::Label,
    _close_button: gtk::Button,
});

media_box_widget!(video_bottom, MediaBoxVideoBottom, "media_box_video_bottom", "/nl/g4d/Girens/media_box_video_bottom.ui", {
    _scale_adjustment: gtk::Adjustment,
    _time_left_label: gtk::Label,
    _time_right_label: gtk::Label,
    _cover_image: gtk::Image,
    _playqueue_button: gtk::MenuButton,
    _skip_backward_button: gtk::Button,
    _prev_button: gtk::Button,
    _play_button: gtk::Button,
    _play_image: gtk::Image,
    _next_button: gtk::Button,
    _skip_forward_button: gtk::Button,
    _tracks_button: gtk::MenuButton,
    _volume_button: gtk::ScaleButton,
});

impl MediaBoxVideoBottom {
    /// In a narrow window only the buttons that are needed most remain;
    /// skipping is still possible with the keyboard and the time bar.
    pub fn set_narrow(&self, narrow: bool) {
        let imp = self.imp();
        if let Some(cover) = imp._cover_image.parent() {
            cover.set_visible(!narrow);
        }
        imp._skip_backward_button.set_visible(!narrow);
        imp._skip_forward_button.set_visible(!narrow);
    }

    /// The button that opens the audio and subtitle choice.
    pub fn tracks_button(&self) -> gtk::MenuButton {
        self.imp()._tracks_button.get()
    }
}

/// Makes a volume button show and change the stored volume, which the
/// player follows.
fn bind_volume(button: &gtk::ScaleButton) {
    gtk::gio::Settings::new(crate::config::APP_ID)
        .bind("volume-level", button, "value")
        .build();
}

/// The controls one widget offers; every part is optional.
#[derive(Default)]
struct Parts {
    revealers: Vec<gtk::Revealer>,
    play_images: Vec<gtk::Image>,
    prev_buttons: Vec<gtk::Button>,
    next_buttons: Vec<gtk::Button>,
    title_labels: Vec<gtk::Label>,
    subtitle_labels: Vec<gtk::Label>,
    adjustments: Vec<gtk::Adjustment>,
    time_left_labels: Vec<gtk::Label>,
    time_right_labels: Vec<gtk::Label>,
    cover_images: Vec<gtk::Image>,
    windowed_buttons: Vec<gtk::Button>,
}

struct Inner {
    player: Player,
    /// Whether these controls are for video (otherwise for music).
    video: bool,
    parts: RefCell<Parts>,
    item: RefCell<Option<Item>>,
    /// Set while the position slider is moved by the player, not the user.
    updating_position: Cell<bool>,
    cover_task: RefCell<Option<glib::JoinHandle<()>>>,
    on_fullscreen: RefCell<Option<Box<dyn Fn()>>>,
    on_windowed: RefCell<Option<Box<dyn Fn()>>>,
    on_popover: RefCell<Option<Box<dyn Fn(bool)>>>,
}

/// Connects control widgets to the player.
#[derive(Clone)]
pub struct MediaControls {
    inner: Rc<Inner>,
}

impl MediaControls {
    pub fn new(player: &Player, video: bool) -> Self {
        let inner = Rc::new(Inner {
            player: player.clone(),
            video,
            parts: RefCell::default(),
            item: RefCell::default(),
            updating_position: Cell::new(false),
            cover_task: RefCell::default(),
            on_fullscreen: RefCell::default(),
            on_windowed: RefCell::default(),
            on_popover: RefCell::default(),
        });
        let weak = Rc::downgrade(&inner);
        player.connect(move |event| {
            if let Some(inner) = weak.upgrade() {
                inner.on_event(event);
            }
        });
        Self { inner }
    }

    pub fn add_music(&self, widget: &MediaBoxMusic) {
        let imp = widget.imp();
        self.add_revealer(&imp._revealer);
        self.add_transport(
            &imp._play_button,
            &imp._play_image,
            &imp._prev_button,
            &imp._next_button,
        );
        self.add_position(
            &imp._scale_adjustment,
            &imp._time_left_label,
            &imp._time_right_label,
        );
        self.add_labels(&imp._title_label, &imp._subtitle_label);
        self.add_close(&imp._close_button);
        self.add_queue_button(&imp._playqueue_button, &imp._cover_image);
        bind_volume(&imp._volume_button);
        // Clicking the title or the cover opens the album of the track.
        for widget in [
            imp._label_box.upcast_ref::<gtk::Widget>(),
            imp._label2_box.upcast_ref(),
            imp._cover_image.upcast_ref(),
        ] {
            let click = gtk::GestureClick::new();
            let weak = Rc::downgrade(&self.inner);
            click.connect_released(move |gesture, _, _, _| {
                let album = weak
                    .upgrade()
                    .and_then(|inner| inner.item.borrow().as_ref()?.parent_rating_key);
                if let (Some(album), Some(widget)) = (album, gesture.widget()) {
                    let _ = widget.activate_action(
                        "win.show-album-by-id",
                        Some(&album.as_str().to_variant()),
                    );
                }
            });
            widget.add_controller(click);
            widget.set_cursor_from_name(Some("pointer"));
        }
        imp._media_settings
            .set_popover(Some(&MusicPopoverMenu::new(&self.inner.player)));
    }

    pub fn add_video_top(&self, widget: &MediaBoxVideoTop) {
        let imp = widget.imp();
        self.add_revealer(&imp._revealer);
        self.add_labels(&imp._title_label, &imp._subtitle_label);
        self.add_close(&imp._close_button);
        let weak = Rc::downgrade(&self.inner);
        imp._fullscreen_button.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                if let Some(callback) = inner.on_fullscreen.borrow().as_ref() {
                    callback();
                }
            }
        });
        let weak = Rc::downgrade(&self.inner);
        imp._fullscreen_windowed_button.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                if let Some(callback) = inner.on_windowed.borrow().as_ref() {
                    callback();
                }
            }
        });
        self.inner
            .parts
            .borrow_mut()
            .windowed_buttons
            .push(imp._fullscreen_windowed_button.get());
    }

    pub fn add_video_bottom(&self, widget: &MediaBoxVideoBottom) {
        let imp = widget.imp();
        self.add_revealer(&imp._revealer);
        self.add_transport(
            &imp._play_button,
            &imp._play_image,
            &imp._prev_button,
            &imp._next_button,
        );
        self.add_position(
            &imp._scale_adjustment,
            &imp._time_left_label,
            &imp._time_right_label,
        );
        self.add_queue_button(&imp._playqueue_button, &imp._cover_image);
        bind_volume(&imp._volume_button);
        let player = self.inner.player.clone();
        imp._skip_backward_button
            .connect_clicked(move |_| player.seek_by(-10));
        let player = self.inner.player.clone();
        imp._skip_forward_button
            .connect_clicked(move |_| player.seek_by(30));
    }

    fn add_revealer(&self, revealer: &gtk::Revealer) {
        revealer.set_reveal_child(false);
        self.inner
            .parts
            .borrow_mut()
            .revealers
            .push(revealer.clone());
    }

    fn add_transport(
        &self,
        play: &gtk::Button,
        play_image: &gtk::Image,
        prev: &gtk::Button,
        next: &gtk::Button,
    ) {
        let player = self.inner.player.clone();
        play.connect_clicked(move |_| player.play_pause());
        let player = self.inner.player.clone();
        prev.connect_clicked(move |_| player.previous());
        let player = self.inner.player.clone();
        next.connect_clicked(move |_| player.next());
        let mut parts = self.inner.parts.borrow_mut();
        parts.play_images.push(play_image.clone());
        parts.prev_buttons.push(prev.clone());
        parts.next_buttons.push(next.clone());
    }

    fn add_position(&self, adjustment: &gtk::Adjustment, left: &gtk::Label, right: &gtk::Label) {
        let weak = Rc::downgrade(&self.inner);
        adjustment.connect_value_changed(move |adjustment| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.updating_position.get() {
                return;
            }
            let duration = inner.duration();
            if duration > 0 {
                inner
                    .player
                    .seek_to((adjustment.value() * duration as f64) as u64);
            }
        });
        let mut parts = self.inner.parts.borrow_mut();
        parts.adjustments.push(adjustment.clone());
        parts.time_left_labels.push(left.clone());
        parts.time_right_labels.push(right.clone());
    }

    fn add_labels(&self, title: &gtk::Label, subtitle: &gtk::Label) {
        let mut parts = self.inner.parts.borrow_mut();
        parts.title_labels.push(title.clone());
        parts.subtitle_labels.push(subtitle.clone());
    }

    fn add_close(&self, button: &gtk::Button) {
        let player = self.inner.player.clone();
        button.connect_clicked(move |_| player.stop());
    }

    fn add_queue_button(&self, button: &gtk::MenuButton, cover: &gtk::Image) {
        let popover = QueuePopover::new(&self.inner.player);
        for shown in [true, false] {
            let weak = Rc::downgrade(&self.inner);
            let notify = move |_: &QueuePopover| {
                if let Some(inner) = weak.upgrade() {
                    if let Some(callback) = inner.on_popover.borrow().as_ref() {
                        callback(shown);
                    }
                }
            };
            if shown {
                popover.connect_show(notify);
            } else {
                popover.connect_hide(notify);
            }
        }
        button.set_popover(Some(&popover));
        self.inner
            .parts
            .borrow_mut()
            .cover_images
            .push(cover.clone());
    }

    pub fn connect_fullscreen(&self, callback: impl Fn() + 'static) {
        self.inner.on_fullscreen.replace(Some(Box::new(callback)));
    }

    pub fn connect_windowed(&self, callback: impl Fn() + 'static) {
        self.inner.on_windowed.replace(Some(Box::new(callback)));
    }

    /// Called when the play queue popover opens (`true`) or closes.
    pub fn connect_popover(&self, callback: impl Fn(bool) + 'static) {
        self.inner.on_popover.replace(Some(Box::new(callback)));
    }

    /// Whether an item for these controls is playing.
    pub fn is_active(&self) -> bool {
        self.inner.item.borrow().is_some()
    }

    pub fn set_revealed(&self, revealed: bool) {
        self.inner.set_revealed(revealed);
    }

    pub fn set_windowed_button_visible(&self, visible: bool) {
        for button in &self.inner.parts.borrow().windowed_buttons {
            button.set_visible(visible);
        }
    }
}

impl Inner {
    fn duration(&self) -> u64 {
        self.player
            .loaded_item()
            .and_then(|item| item.duration)
            .or_else(|| self.item.borrow().as_ref().and_then(|item| item.duration))
            .unwrap_or(0)
    }

    fn set_revealed(&self, revealed: bool) {
        for revealer in &self.parts.borrow().revealers {
            revealer.set_reveal_child(revealed);
        }
    }

    fn on_event(self: &Rc<Self>, event: &Event) {
        match event {
            Event::Started { item, video } => {
                if *video == self.video {
                    self.item.replace(Some((**item).clone()));
                    self.show_item(item);
                    self.show_paused(false);
                    self.show_position(0);
                    self.show_queue_position();
                    self.set_revealed(true);
                } else {
                    self.item.replace(None);
                    self.set_revealed(false);
                }
            }
            Event::Stopped => {
                self.item.replace(None);
                self.set_revealed(false);
            }
            _ if self.item.borrow().is_none() => {}
            Event::Paused(paused) => self.show_paused(*paused),
            Event::Position(position) => self.show_position(*position),
            Event::QueueChanged => self.show_queue_position(),
            Event::AskResume(_) | Event::RepeatChanged(_) => {}
        }
    }

    fn show_item(self: &Rc<Self>, item: &Item) {
        let (title, subtitle) = match item.kind {
            ItemKind::Episode => (
                item.grandparent_title.clone().unwrap_or_default(),
                match item.season_episode() {
                    Some(number) => format!("{number} - {}", item.title),
                    None => item.title.clone(),
                },
            ),
            ItemKind::Track => (
                item.title.clone(),
                format!(
                    "{} - {}",
                    item.grandparent_title.as_deref().unwrap_or_default(),
                    item.parent_title.as_deref().unwrap_or_default()
                ),
            ),
            _ => (
                item.title.clone(),
                item.year.map(|year| year.to_string()).unwrap_or_default(),
            ),
        };
        let parts = self.parts.borrow();
        for label in &parts.title_labels {
            label.set_text(&title);
        }
        for label in &parts.subtitle_labels {
            label.set_text(&subtitle);
        }

        if parts.cover_images.is_empty() {
            return;
        }
        for picture in &parts.cover_images {
            picture.set_paintable(gdk::Paintable::NONE);
        }
        let Some(thumb) = item.cover().map(str::to_owned) else {
            return;
        };
        let session = Session::current();
        let (server, machine) = (session.server(), session.machine_identifier());
        // The cache is keyed by the item whose image it is.
        let key = match item.kind {
            ItemKind::Episode => item.grandparent_rating_key.unwrap_or(item.rating_key),
            _ => item.rating_key,
        };
        let weak = Rc::downgrade(self);
        let task = glib::spawn_future_local(async move {
            // Without a connection only covers seen before are available.
            let texture = match (&server, &machine) {
                (Some(server), _) => images::cover(server, key, &thumb).await,
                (None, Some(machine)) => images::cached(machine, key).await,
                (None, None) => None,
            };
            if let (Some(inner), Some(texture)) = (weak.upgrade(), texture) {
                for picture in &inner.parts.borrow().cover_images {
                    picture.set_paintable(Some(&texture));
                }
            }
        });
        if let Some(previous) = self.cover_task.replace(Some(task)) {
            previous.abort();
        }
    }

    fn show_paused(&self, paused: bool) {
        let icon = if paused {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        };
        for image in &self.parts.borrow().play_images {
            image.set_icon_name(Some(icon));
        }
    }

    fn show_position(&self, position: u64) {
        let duration = self.duration();
        let parts = self.parts.borrow();
        self.updating_position.set(true);
        if duration > 0 {
            for adjustment in &parts.adjustments {
                adjustment.set_value((position as f64 / duration as f64).min(1.0));
            }
        }
        self.updating_position.set(false);
        let elapsed = util::format_duration(position);
        let left = format!(
            "-{}",
            util::format_duration(duration.saturating_sub(position))
        );
        for label in &parts.time_left_labels {
            label.set_text(&elapsed);
        }
        for label in &parts.time_right_labels {
            label.set_text(&left);
        }
    }

    fn show_queue_position(&self) {
        let parts = self.parts.borrow();
        for button in &parts.prev_buttons {
            button.set_sensitive(self.player.has_previous());
        }
        for button in &parts.next_buttons {
            button.set_sensitive(self.player.has_next());
        }
    }
}
