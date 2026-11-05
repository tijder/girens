//! Search results across all libraries.

use std::cell::RefCell;

use adw::subclass::prelude::*;
use girens_plex::model::ItemKind;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use super::List;
use crate::runtime;
use crate::session::Session;

const KINDS: [ItemKind; 6] = [
    ItemKind::Movie,
    ItemKind::Show,
    ItemKind::Episode,
    ItemKind::Season,
    ItemKind::Artist,
    ItemKind::Album,
];
const RESULTS_PER_KIND: u32 = 10;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/search_view.ui")]
    pub struct SearchView {
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _section_flow: TemplateChild<List>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchView {
        const NAME: &'static str = "search_view";
        type Type = super::SearchView;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            List::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SearchView {
        fn constructed(&self) {
            self.parent_constructed();
            self._section_flow.set_grid_mode();
        }
    }

    impl WidgetImpl for SearchView {}
    impl BoxImpl for SearchView {}
}

glib::wrapper! {
    pub struct SearchView(ObjectSubclass<imp::SearchView>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SearchView {
    pub fn set_cover_width(&self, width: i32) {
        self.imp()._section_flow.set_cover_width(width);
    }

    /// Shows the new state of an item, e.g. after it was marked as played.
    pub fn update_item(&self, item: &girens_plex::model::Item) {
        self.imp()._section_flow.update_item(item);
    }

    /// Searches for `query`; results of an earlier search are dropped.
    pub fn search(&self, query: &str) {
        let imp = self.imp();
        imp._title_label.set_label(query);
        imp._section_flow.clear();
        let Some(server) = Session::current().server() else {
            return;
        };
        let query = query.to_owned();

        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                // One request per kind, shown as they arrive.
                let mut searches: Vec<_> = KINDS
                    .into_iter()
                    .map(|kind| {
                        let (server, query) = (server.clone(), query.clone());
                        Box::pin(runtime::run(async move {
                            server.search(&query, kind, RESULTS_PER_KIND).await
                        }))
                    })
                    .collect();
                for search in &mut searches {
                    match search.await {
                        Ok(items) => view.imp()._section_flow.append(items),
                        Err(err) => {
                            crate::notify::failure(&crate::i18n::gettext("Search failed"), &err)
                        }
                    }
                }
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }
}
