//! One track in an album.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use girens_model::model::Item;
use gtk::prelude::*;
use gtk::{gio, glib, CompositeTemplate};

use crate::i18n::gettext;
use crate::util;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/album_item.ui")]
    pub struct AlbumItem {
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _index_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _index_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub _playing_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _music_clip_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _menu_button: TemplateChild<gtk::MenuButton>,
        /// The track the row shows.
        pub track: RefCell<Option<Item>>,
        /// Whether the menu leads to the album: in a playlist, where the
        /// tracks come from anywhere.
        pub album_link: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumItem {
        const NAME: &'static str = "album_item";
        type Type = super::AlbumItem;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AlbumItem {
        fn constructed(&self) {
            self.parent_constructed();
            let row = self.obj();

            // The menu depends on the track and on what is playing, so it is
            // made when it is asked for.
            self._menu_button.set_create_popup_func(glib::clone!(
                #[weak]
                row,
                move |button| button.set_menu_model(row.menu().as_ref())
            ));
            self._menu_button.connect_active_notify(|button| {
                if button.is_active() {
                    button.add_css_class("open");
                } else {
                    button.remove_css_class("open");
                }
            });

            let click = gtk::GestureClick::builder()
                .button(gtk::gdk::BUTTON_SECONDARY)
                .build();
            click.connect_pressed(glib::clone!(
                #[weak]
                row,
                move |click, _, _, _| {
                    click.set_state(gtk::EventSequenceState::Claimed);
                    row.imp()._menu_button.popup();
                }
            ));
            row.add_controller(click);

            // On a touch screen there is no secondary click.
            let press = gtk::GestureLongPress::builder().touch_only(true).build();
            press.connect_pressed(glib::clone!(
                #[weak]
                row,
                move |press, _, _| {
                    press.set_state(gtk::EventSequenceState::Claimed);
                    row.imp()._menu_button.popup();
                }
            ));
            row.add_controller(press);
        }
    }
    impl WidgetImpl for AlbumItem {}
    impl BoxImpl for AlbumItem {}
}

glib::wrapper! {
    pub struct AlbumItem(ObjectSubclass<imp::AlbumItem>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl AlbumItem {
    pub fn new(track: &Item, album_link: bool) -> Self {
        let row: Self = glib::Object::new();
        row.set_track(track, album_link);
        row
    }

    /// Shows `track`; the row can be reused for another one at any time.
    /// With `album_link` its menu leads to the album it is on.
    pub fn set_track(&self, track: &Item, album_link: bool) {
        let imp = self.imp();
        imp.album_link.set(album_link);
        if track.view_count == 0 {
            imp._index_label.add_css_class("unplayed");
        } else {
            imp._index_label.remove_css_class("unplayed");
        }
        imp._title_label.set_text(&track.title);
        imp.track.replace(Some(track.clone()));
        self.mark_playing();
        imp._index_label.set_text(
            &track
                .index
                .map(|index| index.to_string())
                .unwrap_or_default(),
        );
        imp._time_label
            .set_text(&util::format_duration(track.duration.unwrap_or(0)));

        // A track with a music video gets a button to play that instead.
        let clip = track
            .primary_extra_key
            .as_deref()
            .and_then(util::rating_key_from_path);
        imp._music_clip_button.set_visible(clip.is_some());
        match clip {
            Some(clip) => {
                imp._music_clip_button
                    .set_action_target_value(Some(&clip.as_str().to_variant()));
                imp._music_clip_button
                    .set_action_name(Some("win.play-item"));
            }
            None => imp._music_clip_button.set_action_name(None),
        }
    }

    /// What can be done with the track, for its menu.
    fn menu(&self) -> Option<gio::Menu> {
        let imp = self.imp();
        let track = imp.track.borrow();
        let track = track.as_ref()?;
        let key = track.rating_key.as_str().to_variant();
        let section = |entries: &[(String, &str, &glib::Variant)]| {
            let section = gio::Menu::new();
            for (label, action, target) in entries {
                let entry = gio::MenuItem::new(Some(label), None);
                entry.set_action_and_target_value(Some(action), Some(target));
                section.append_item(&entry);
            }
            section
        };

        let menu = gio::Menu::new();
        if crate::player::can_enqueue() {
            menu.append_section(
                None,
                &section(&[
                    (gettext("Play next"), "win.queue-next-by-id", &key),
                    (gettext("Add to play queue"), "win.queue-add-by-id", &key),
                ]),
            );
        }
        let mut entries = vec![(
            gettext("Add to playlist…"),
            "win.add-to-playlist-by-id",
            key.clone(),
        )];
        if let (true, Some(album)) = (imp.album_link.get(), track.parent_rating_key) {
            entries.push((
                gettext("Go to album view"),
                "win.show-album-by-id",
                album.as_str().to_variant(),
            ));
        }
        let entries: Vec<_> = entries
            .iter()
            .map(|(label, action, target)| (label.clone(), *action, target))
            .collect();
        menu.append_section(None, &section(&entries));
        menu.append_section(
            None,
            &section(&[if track.view_count == 0 {
                (gettext("Mark as Played"), "win.mark-as-played-by-id", &key)
            } else {
                (gettext("Mark as Unplayed"), "win.mark-as-unplayed-by-id", &key)
            }]),
        );
        Some(menu)
    }

    /// Marks the row when its track is the one that is playing, and tells
    /// whether it plays or waits.
    pub fn mark_playing(&self) {
        let imp = self.imp();
        let track = imp.track.borrow().as_ref().map(|track| track.rating_key);
        if track.is_some() && crate::player::now_playing() == track {
            imp._title_label.add_css_class("now-playing");
            imp._playing_image
                .set_icon_name(Some(if crate::player::now_paused() {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                }));
            imp._index_stack.set_visible_child(&*imp._playing_image);
        } else {
            imp._title_label.remove_css_class("now-playing");
            imp._index_stack.set_visible_child(&*imp._index_label);
        }
    }
}
