//! Home: on deck and recently added.

use std::cell::RefCell;

use adw::subclass::prelude::*;
use girens_model::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use super::List;
use crate::runtime;
use crate::session::Session;

/// How many empty tiles a row shows while it is loading.
const PLACEHOLDERS: usize = 8;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/discover_view.ui")]
    pub struct DiscoverView {
        #[template_child]
        pub _label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _seasons_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _movies_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _music_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _deck_shows_box: TemplateChild<List>,
        #[template_child]
        pub _movies_shows_box: TemplateChild<List>,
        #[template_child]
        pub _seasons_shows_box: TemplateChild<List>,
        #[template_child]
        pub _music_shows_box: TemplateChild<List>,
        pub refresh_task: RefCell<Option<glib::JoinHandle<()>>>,
        /// The server whose items the rows show, once they were loaded.
        pub loaded_from: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DiscoverView {
        const NAME: &'static str = "discover_view";
        type Type = super::DiscoverView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            List::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DiscoverView {}
    impl WidgetImpl for DiscoverView {}
    impl BinImpl for DiscoverView {}
}

glib::wrapper! {
    pub struct DiscoverView(ObjectSubclass<imp::DiscoverView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl DiscoverView {
    fn lists(&self) -> [List; 4] {
        let imp = self.imp();
        [
            imp._deck_shows_box.get(),
            imp._movies_shows_box.get(),
            imp._seasons_shows_box.get(),
            imp._music_shows_box.get(),
        ]
    }

    pub fn set_cover_width(&self, width: i32) {
        for list in self.lists() {
            list.set_cover_width(width);
        }
    }

    /// Shows the new state of an item, e.g. after it was marked as played.
    pub fn update_item(&self, item: &girens_model::model::Item) {
        for list in self.lists() {
            list.update_item(item);
        }
    }

    /// Heading and list of each row.
    fn rows(&self) -> [(gtk::Label, List); 4] {
        let imp = self.imp();
        [
            (imp._label.get(), imp._deck_shows_box.get()),
            (imp._seasons_label.get(), imp._seasons_shows_box.get()),
            (imp._movies_label.get(), imp._movies_shows_box.get()),
            (imp._music_label.get(), imp._music_shows_box.get()),
        ]
    }

    /// Height of the images in each row, in the order of `rows`.
    const IMAGE_HEIGHTS: [i32; 4] = [300, 300, 300, 200];

    /// Fills a row; a row without items is hidden together with its heading.
    fn fill(heading: &gtk::Label, list: &List, items: Vec<Item>) {
        let filled = !items.is_empty();
        list.set_items(items);
        heading.set_visible(filled);
        list.set_visible(filled);
    }

    /// Reloads all rows from the connected server.
    pub fn refresh(&self) {
        let imp = self.imp();
        let Some(server) = Session::current().server() else {
            return;
        };
        // Rows that already show this server keep their items until the new
        // ones are in; otherwise empty tiles take their place, so that the
        // page does not jump when the items arrive.
        let machine = Session::current().machine_identifier();
        if machine.is_none() || *imp.loaded_from.borrow() != machine {
            for ((heading, list), image_height) in self.rows().iter().zip(Self::IMAGE_HEIGHTS) {
                heading.set_visible(true);
                list.set_visible(true);
                list.set_placeholders(PLACEHOLDERS, image_height);
            }
        }

        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let (deck, latest) =
                    runtime::run(
                        async move { tokio::join!(server.on_deck(), server.recently_added()) },
                    )
                    .await;
                let [deck_row, seasons_row, movies_row, music_row] = view.rows();
                let retry = || {
                    glib::clone!(
                        #[weak]
                        view,
                        move || view.refresh()
                    )
                };

                match deck {
                    Ok(items) => Self::fill(&deck_row.0, &deck_row.1, items),
                    Err(err) => {
                        Self::fill(&deck_row.0, &deck_row.1, Vec::new());
                        crate::notify::failure_with_retry(
                            &crate::messages::load_on_deck(),
                            &err,
                            retry(),
                        );
                    }
                }
                let latest = latest.unwrap_or_else(|err| {
                    crate::notify::failure_with_retry(
                        &crate::messages::load_recently_added(),
                        &err,
                        retry(),
                    );
                    Vec::new()
                });
                let of = |kinds: &[ItemKind]| -> Vec<Item> {
                    latest
                        .iter()
                        .filter(|item| kinds.contains(&item.kind))
                        .cloned()
                        .collect()
                };
                Self::fill(
                    &seasons_row.0,
                    &seasons_row.1,
                    // Plex lists the new seasons and episodes, Ister the shows.
                    of(&[ItemKind::Episode, ItemKind::Season, ItemKind::Show]),
                );
                Self::fill(&movies_row.0, &movies_row.1, of(&[ItemKind::Movie]));
                Self::fill(&music_row.0, &music_row.1, of(&[ItemKind::Album]));
                view.imp().loaded_from.replace(machine);
            }
        ));
        if let Some(previous) = imp.refresh_task.replace(Some(task)) {
            previous.abort();
        }
    }
}
