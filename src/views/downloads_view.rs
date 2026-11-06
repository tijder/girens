//! The items that have an offline copy. They can be shown and played without
//! a connection to the server.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use super::List;
use crate::i18n::gettext;
use crate::session::Session;
use crate::sync;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DownloadsView {
        pub list: List,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DownloadsView {
        const NAME: &'static str = "GirensDownloadsView";
        type Type = super::DownloadsView;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for DownloadsView {
        fn constructed(&self) {
            self.parent_constructed();
            let title = gtk::Label::builder()
                .label(gettext("Downloads"))
                .halign(gtk::Align::Start)
                .margin_top(10)
                .margin_bottom(15)
                .margin_start(10)
                .build();
            title.add_css_class("title-1");
            self.list.set_grid_mode();

            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.set_vexpand(true);
            content.append(&title);
            content.append(&self.list);
            self.obj().set_child(Some(&content));
        }
    }

    impl WidgetImpl for DownloadsView {}
    impl BinImpl for DownloadsView {}
}

glib::wrapper! {
    pub struct DownloadsView(ObjectSubclass<imp::DownloadsView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for DownloadsView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl DownloadsView {
    /// Reads the offline copies from the disk again.
    pub fn refresh(&self) {
        let list = &self.imp().list;
        let session = Session::current();
        let items = match (session.machine_identifier(), session.user_key()) {
            (Some(machine), Some(user)) => sync::offline_items(&machine, &user),
            _ => Vec::new(),
        };
        let empty = items.is_empty();
        list.set_items(items);
        list.set_empty_message(empty.then(|| gettext("Nothing downloaded yet")).as_deref());
    }

    pub fn set_cover_width(&self, width: i32) {
        self.imp().list.set_cover_width(width);
    }
}
