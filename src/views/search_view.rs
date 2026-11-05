//! Search results across all libraries, one row per kind of item.

use std::cell::RefCell;

use adw::subclass::prelude::*;
use girens_plex::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use super::List;
use crate::i18n::gettext;
use crate::runtime;
use crate::session::Session;

const RESULTS_PER_KIND: u32 = 10;

/// The kinds that are searched, with the heading of their row.
fn kinds() -> [(ItemKind, String); 6] {
    [
        (ItemKind::Movie, gettext("Movies")),
        (ItemKind::Show, gettext("Shows")),
        (ItemKind::Season, gettext("Seasons")),
        (ItemKind::Episode, gettext("Episodes")),
        (ItemKind::Artist, gettext("Artists")),
        (ItemKind::Album, gettext("Albums")),
    ]
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/search_view.ui")]
    pub struct SearchView {
        #[template_child]
        pub _stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub _results_box: TemplateChild<gtk::Box>,
        /// Heading and row of each kind, in the order of [`kinds`].
        pub rows: RefCell<Vec<(gtk::Label, List)>>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
        pub query: RefCell<String>,
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
            let mut rows = Vec::new();
            for (_, title) in kinds() {
                let heading = gtk::Label::builder()
                    .label(title)
                    .halign(gtk::Align::Start)
                    .margin_top(20)
                    .margin_bottom(5)
                    .margin_start(5)
                    .visible(false)
                    .build();
                heading.add_css_class("title-3");
                let list: List = glib::Object::new();
                list.set_visible(false);
                self._results_box.append(&heading);
                self._results_box.append(&list);
                rows.push((heading, list));
            }
            self.rows.replace(rows);
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
    /// What was searched for last.
    pub fn query(&self) -> String {
        self.imp().query.borrow().clone()
    }

    pub fn set_cover_width(&self, width: i32) {
        for (_, list) in self.imp().rows.borrow().iter() {
            list.set_cover_width(width);
        }
    }

    /// Shows the new state of an item, e.g. after it was marked as played.
    pub fn update_item(&self, item: &Item) {
        for (_, list) in self.imp().rows.borrow().iter() {
            list.update_item(item);
        }
    }

    fn show_row(&self, index: usize, items: Vec<Item>) {
        if let Some((heading, list)) = self.imp().rows.borrow().get(index) {
            let found = !items.is_empty();
            list.set_items(items);
            heading.set_visible(found);
            list.set_visible(found);
        }
    }

    /// Searches for `query`; results of an earlier search are dropped.
    pub fn search(&self, query: &str) {
        let imp = self.imp();
        imp.query.replace(query.to_owned());
        for index in 0..kinds().len() {
            self.show_row(index, Vec::new());
        }
        let Some(server) = Session::current().server() else {
            return;
        };
        imp._stack.set_visible_child_name("loading");
        let query = query.to_owned();

        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                // One request per kind, all under way at once.
                let mut searches: Vec<_> = kinds()
                    .into_iter()
                    .map(|(kind, _)| {
                        let (server, query) = (server.clone(), query.clone());
                        Box::pin(runtime::run(async move {
                            server.search(&query, kind, RESULTS_PER_KIND).await
                        }))
                    })
                    .collect();
                let mut found = false;
                for (index, search) in searches.iter_mut().enumerate() {
                    match search.await {
                        Ok(items) => {
                            if !items.is_empty() {
                                found = true;
                                view.imp()._stack.set_visible_child_name("results");
                            }
                            view.show_row(index, items);
                        }
                        Err(err) => {
                            let retry = glib::clone!(
                                #[weak]
                                view,
                                #[strong]
                                query,
                                move || view.search(&query)
                            );
                            crate::notify::failure_with_retry(
                                &crate::messages::search(),
                                &err,
                                retry,
                            );
                        }
                    }
                }
                if !found {
                    view.imp()._stack.set_visible_child_name("empty");
                }
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }
}
