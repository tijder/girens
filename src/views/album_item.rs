//! One track in an album.

use adw::subclass::prelude::*;
use girens_model::model::Item;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

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

    impl ObjectImpl for AlbumItem {}
    impl WidgetImpl for AlbumItem {}
    impl BoxImpl for AlbumItem {}
}

glib::wrapper! {
    pub struct AlbumItem(ObjectSubclass<imp::AlbumItem>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl AlbumItem {
    pub fn new(track: &Item) -> Self {
        let row: Self = glib::Object::new();
        row.set_track(track);
        row
    }

    /// Shows `track`; the row can be reused for another one at any time.
    pub fn set_track(&self, track: &Item) {
        let imp = self.imp();
        if track.view_count == 0 {
            imp._index_label.add_css_class("unplayed");
        } else {
            imp._index_label.remove_css_class("unplayed");
        }
        imp._title_label.set_text(&track.title);
        if crate::player::now_playing() == Some(track.rating_key) {
            imp._title_label.add_css_class("now-playing");
            imp._index_stack.set_visible_child(&*imp._playing_image);
        } else {
            imp._title_label.remove_css_class("now-playing");
            imp._index_stack.set_visible_child(&*imp._index_label);
        }
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
}
