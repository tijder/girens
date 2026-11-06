//! Offline sync: the dialog with synced items, the options for syncing a
//! show, and the popover that lists running downloads.

use girens_model::ItemId;
use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_model::model::{Item, ItemKind};
use gtk::{glib, CompositeTemplate};

use crate::session::Session;
use crate::store::SyncEntry;
use crate::sync::{Event, Sync};
use crate::{images, runtime};

/// Loads the cover of `item` into `image`.
fn load_cover(image: &gtk::Image, item: &Item) {
    let (Some(thumb), Some(server)) =
        (item.cover().map(str::to_owned), Session::current().server())
    else {
        return;
    };
    let key = match item.kind {
        ItemKind::Episode => item.grandparent_rating_key.unwrap_or(item.rating_key),
        _ => item.rating_key,
    };
    glib::spawn_future_local(glib::clone!(
        #[weak]
        image,
        async move {
            if let Some(texture) = images::cover(&server, key, &thumb).await {
                image.set_paintable(Some(&texture));
            }
        }
    ));
}

/// `Show - s01e02 - Title` for episodes, the title otherwise.
fn display_title(item: &Item) -> String {
    match (
        item.kind,
        item.grandparent_title.as_deref(),
        item.season_episode(),
    ) {
        (ItemKind::Episode, Some(show), Some(number)) => {
            format!("{show} - {number} - {}", item.title)
        }
        (ItemKind::Movie, _, _) => match item.year {
            Some(year) => format!("{} ({year})", item.title),
            None => item.title.clone(),
        },
        _ => item.title.clone(),
    }
}

// One synced item

mod sync_item {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/sync_item.ui")]
    pub struct SyncItem {
        #[template_child]
        pub _cover_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _watched_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _remove_button: TemplateChild<gtk::Button>,
        pub rating_key: Cell<ItemId>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SyncItem {
        const NAME: &'static str = "sync_item";
        type Type = super::SyncItem;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SyncItem {}
    impl WidgetImpl for SyncItem {}
    impl BoxImpl for SyncItem {}
}

glib::wrapper! {
    pub struct SyncItem(ObjectSubclass<sync_item::SyncItem>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SyncItem {
    fn new(entry: &SyncEntry) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        let key = entry.rating_key;
        imp.rating_key.set(key);
        imp._title_label.set_text("");
        imp._watched_image.set_visible(false);
        imp._remove_button
            .connect_clicked(move |_| Sync::current().remove(key));

        if let Some(server) = Session::current().server() {
            glib::spawn_future_local(glib::clone!(
                #[weak]
                row,
                async move {
                    match runtime::run(async move { server.item(key).await }).await {
                        Ok(item) => {
                            let imp = row.imp();
                            imp._title_label.set_text(&display_title(&item));
                            let video = matches!(item.kind, ItemKind::Movie | ItemKind::Episode);
                            imp._watched_image.set_visible(video && !item.is_watched());
                            load_cover(&imp._cover_image, &item);
                        }
                        Err(err) => {
                            crate::notify::failure(&crate::messages::load_synced_item(), &err)
                        }
                    }
                }
            ));
        }
        row
    }
}

// The dialog with all synced items

mod sync_dialog {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/sync_dialog.ui")]
    pub struct SyncDialog {
        #[template_child]
        pub _item_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub _sync_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SyncDialog {
        const NAME: &'static str = "sync_dialog";
        type Type = super::SyncDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SyncDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let dialog = self.obj();
            self._sync_button.connect_clicked(glib::clone!(
                #[weak]
                dialog,
                move |_| {
                    Sync::current().run();
                    dialog.close();
                }
            ));
            Sync::current().connect(glib::clone!(
                #[weak]
                dialog,
                move |event| {
                    if matches!(event, Event::EntriesChanged) && dialog.is_mapped() {
                        dialog.fill();
                    }
                }
            ));
            dialog.connect_map(|dialog| dialog.fill());
        }
    }

    impl WidgetImpl for SyncDialog {}
    impl AdwDialogImpl for SyncDialog {}
}

glib::wrapper! {
    pub struct SyncDialog(ObjectSubclass<sync_dialog::SyncDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl SyncDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn fill(&self) {
        let list = &self.imp()._item_box;
        list.remove_all();
        for entry in Sync::current().entries() {
            list.append(&SyncItem::new(&entry));
        }
    }
}

// Options when syncing a show

mod sync_settings {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/sync_settings.ui")]
    pub struct SyncSettings {
        #[template_child]
        pub _convert_button: TemplateChild<gtk::Switch>,
        #[template_child]
        pub _unwatched_button: TemplateChild<gtk::Switch>,
        #[template_child]
        pub _max_items_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub _sync_button: TemplateChild<gtk::Button>,
        pub rating_key: Cell<ItemId>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SyncSettings {
        const NAME: &'static str = "sync_settings";
        type Type = super::SyncSettings;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SyncSettings {
        fn constructed(&self) {
            self.parent_constructed();
            let window = self.obj();
            self._sync_button.connect_clicked(glib::clone!(
                #[weak]
                window,
                move |_| {
                    let imp = window.imp();
                    // An empty or invalid number means no limit.
                    let max_items = imp._max_items_entry.text().trim().parse().ok();
                    Sync::current().add(SyncEntry {
                        rating_key: imp.rating_key.get(),
                        converted: imp._convert_button.is_active(),
                        only_unwatched: imp._unwatched_button.is_active(),
                        max_items,
                    });
                    window.close();
                }
            ));
        }
    }

    impl WidgetImpl for SyncSettings {}
    impl AdwDialogImpl for SyncSettings {}
}

glib::wrapper! {
    pub struct SyncSettings(ObjectSubclass<sync_settings::SyncSettings>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl SyncSettings {
    pub fn new(rating_key: ItemId) -> Self {
        let dialog: Self = glib::Object::new();
        dialog.imp().rating_key.set(rating_key);
        dialog
    }
}

// Running downloads

mod download_row {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/download_row.ui")]
    pub struct DownloadRow {
        #[template_child]
        pub _cover_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _watched_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _download_spinner: TemplateChild<adw::Spinner>,
        pub rating_key: Cell<ItemId>,
        pub title: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DownloadRow {
        const NAME: &'static str = "download_row";
        type Type = super::DownloadRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DownloadRow {}
    impl WidgetImpl for DownloadRow {}
    impl BoxImpl for DownloadRow {}
}

glib::wrapper! {
    pub struct DownloadRow(ObjectSubclass<download_row::DownloadRow>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl DownloadRow {
    fn new(item: &Item) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        imp.rating_key.set(item.rating_key);
        imp.title.replace(display_title(item));
        imp._title_label.set_text(&imp.title.borrow());
        imp._watched_image.set_visible(false);
        load_cover(&imp._cover_image, item);
        row
    }

    /// Shows how far the download is.
    fn set_progress(&self, fraction: f64) {
        let imp = self.imp();
        let percent = (fraction * 100.0).clamp(0.0, 100.0) as u32;
        imp._title_label
            .set_text(&format!("{percent}% · {}", imp.title.borrow()));
    }

    fn set_finished(&self) {
        let imp = self.imp();
        imp._download_spinner.set_visible(false);
        imp._title_label.set_text(&imp.title.borrow());
    }
}

mod download_menu {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/download_menu.ui")]
    pub struct DownloadMenu {
        #[template_child]
        pub _item_box: TemplateChild<gtk::Box>,
        pub rows: RefCell<Vec<DownloadRow>>,
        pub on_first_download: RefCell<Option<Box<dyn Fn()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DownloadMenu {
        const NAME: &'static str = "download_menu";
        type Type = super::DownloadMenu;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DownloadMenu {
        fn constructed(&self) {
            self.parent_constructed();
            let menu = self.obj();
            Sync::current().connect(glib::clone!(
                #[weak]
                menu,
                move |event| {
                    let imp = menu.imp();
                    match event {
                        Event::Downloading(item, true) => {
                            let row = DownloadRow::new(item);
                            imp._item_box.append(&row);
                            imp.rows.borrow_mut().push(row);
                            if let Some(callback) = imp.on_first_download.borrow().as_ref() {
                                callback();
                            }
                        }
                        Event::Downloading(item, false) => {
                            for row in imp.rows.borrow().iter() {
                                if row.imp().rating_key.get() == item.rating_key {
                                    row.set_finished();
                                }
                            }
                        }
                        Event::Progress(key, Some(fraction)) => {
                            for row in imp.rows.borrow().iter() {
                                if row.imp().rating_key.get() == *key {
                                    row.set_progress(*fraction);
                                }
                            }
                        }
                        // Also ends the rows of a cancelled run.
                        Event::Busy(false) => {
                            for row in imp.rows.borrow().iter() {
                                row.set_finished();
                            }
                        }
                        _ => {}
                    }
                }
            ));
        }
    }

    impl WidgetImpl for DownloadMenu {}
    impl PopoverImpl for DownloadMenu {}
}

glib::wrapper! {
    pub struct DownloadMenu(ObjectSubclass<download_menu::DownloadMenu>)
        @extends gtk::Popover, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
                    gtk::ShortcutManager;
}

impl DownloadMenu {
    /// `on_download` is called whenever a download starts.
    pub fn new(on_download: impl Fn() + 'static) -> Self {
        let menu: Self = glib::Object::new();
        menu.imp()
            .on_first_download
            .replace(Some(Box::new(on_download)));
        menu
    }
}
