//! Home: on deck and recently added.

use std::cell::RefCell;

use adw::subclass::prelude::*;
use girens_plex::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use super::List;
use crate::runtime;
use crate::session::Session;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/discover_view.ui")]
    pub struct DiscoverView {
        #[template_child]
        pub _deck_shows_box: TemplateChild<List>,
        #[template_child]
        pub _movies_shows_box: TemplateChild<List>,
        #[template_child]
        pub _seasons_shows_box: TemplateChild<List>,
        #[template_child]
        pub _music_shows_box: TemplateChild<List>,
        pub refresh_task: RefCell<Option<glib::JoinHandle<()>>>,
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
    pub fn update_item(&self, item: &girens_plex::model::Item) {
        for list in self.lists() {
            list.update_item(item);
        }
    }

    /// Reloads all rows from the connected server.
    pub fn refresh(&self) {
        let imp = self.imp();
        let Some(server) = Session::current().server() else {
            return;
        };
        for list in [
            &imp._deck_shows_box,
            &imp._movies_shows_box,
            &imp._seasons_shows_box,
            &imp._music_shows_box,
        ] {
            list.clear();
        }

        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let imp = view.imp();
                let (deck, latest) =
                    runtime::run(
                        async move { tokio::join!(server.on_deck(), server.recently_added()) },
                    )
                    .await;

                match deck {
                    Ok(items) => imp._deck_shows_box.set_items(items),
                    Err(err) => crate::notify::failure(
                        &crate::i18n::gettext("Could not load on deck"),
                        &err,
                    ),
                }
                match latest {
                    Ok(items) => {
                        let of = |kinds: &[ItemKind]| -> Vec<Item> {
                            items
                                .iter()
                                .filter(|item| kinds.contains(&item.kind))
                                .cloned()
                                .collect()
                        };
                        imp._movies_shows_box.set_items(of(&[ItemKind::Movie]));
                        imp._seasons_shows_box
                            .set_items(of(&[ItemKind::Episode, ItemKind::Season]));
                        imp._music_shows_box.set_items(of(&[ItemKind::Album]));
                    }
                    Err(err) => crate::notify::failure(
                        &crate::i18n::gettext("Could not load recently added"),
                        &err,
                    ),
                }
            }
        ));
        if let Some(previous) = imp.refresh_task.replace(Some(task)) {
            previous.abort();
        }
    }
}
