//! The grid of one library, or of the playlists.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use girens_plex::model::LibrarySection;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use super::List;
use crate::i18n::gettext;
use crate::session::Session;
use crate::{runtime, store};

const PAGE_SIZE: u32 = 100;

/// Translated name of a sort key; unknown keys are shown as they are.
fn sort_label(key: &str) -> String {
    let msgid = match key {
        "addedAt" => "Added at",
        "lastViewedAt" => "Last viewed at",
        "originallyAvailableAt" => "Originally available at",
        "titleSort" => "Title",
        "rating" => "Rating",
        "unwatched" => "Unwatched",
        "viewCount" => "View count",
        "userRating" => "User rating",
        "mediaHeight" => "Media height",
        "duration" => "Duration",
        other => return other.to_owned(),
    };
    gettext(msgid)
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/section_view.ui")]
    pub struct SectionView {
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _section_flow: TemplateChild<List>,
        #[template_child]
        pub _filter_box: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub _section_controll_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub _play_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _shuffle_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _order_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _order_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _load_box: TemplateChild<gtk::Box>,

        /// `None` while the playlists are shown.
        pub section: RefCell<Option<LibrarySection>>,
        pub sort_keys: RefCell<Vec<String>>,
        pub sort: RefCell<Option<String>>,
        pub descending: Cell<bool>,
        /// Number of items requested so far and the size of the library.
        pub offset: Cell<u32>,
        pub total: Cell<u32>,
        pub loading: Cell<bool>,
        pub updating_sorts: Cell<bool>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
        pub on_play: RefCell<Option<Box<dyn Fn(LibrarySection, Option<String>, bool)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SectionView {
        const NAME: &'static str = "section_view";
        type Type = super::SectionView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            List::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SectionView {
        fn constructed(&self) {
            self.parent_constructed();
            let view = self.obj();
            self._section_flow.set_grid_mode();
            self._load_box.set_visible(false);

            self._section_flow.connect_end_reached(glib::clone!(
                #[weak]
                view,
                move || view.load_more()
            ));

            self._filter_box.connect_selected_notify(glib::clone!(
                #[weak]
                view,
                move |dropdown| {
                    let imp = view.imp();
                    if imp.updating_sorts.get() {
                        return;
                    }
                    let key = imp
                        .sort_keys
                        .borrow()
                        .get(dropdown.selected() as usize)
                        .cloned();
                    if key.is_some() && key != *imp.sort.borrow() {
                        imp.sort.replace(key);
                        view.sort_changed();
                    }
                }
            ));

            self._order_button.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| {
                    let imp = view.imp();
                    imp.descending.set(!imp.descending.get());
                    view.sort_changed();
                }
            ));

            for (button, shuffle) in [(&self._play_button, false), (&self._shuffle_button, true)] {
                button.connect_clicked(glib::clone!(
                    #[weak]
                    view,
                    move |_| {
                        let imp = view.imp();
                        let Some(section) = imp.section.borrow().clone() else {
                            return;
                        };
                        if let Some(callback) = imp.on_play.borrow().as_ref() {
                            callback(section, view.sort_parameter(), shuffle);
                        }
                    }
                ));
            }
        }
    }

    impl WidgetImpl for SectionView {}
    impl BinImpl for SectionView {}
}

glib::wrapper! {
    pub struct SectionView(ObjectSubclass<imp::SectionView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SectionView {
    /// Called with the library, its sort order and whether to shuffle when
    /// the play or shuffle button is used.
    pub fn connect_play(&self, callback: impl Fn(LibrarySection, Option<String>, bool) + 'static) {
        self.imp().on_play.replace(Some(Box::new(callback)));
    }

    pub fn set_cover_width(&self, width: i32) {
        self.imp()._section_flow.set_cover_width(width);
    }

    /// Shows the new state of an item, e.g. after it was marked as played.
    pub fn update_item(&self, item: &girens_plex::model::Item) {
        self.imp()._section_flow.update_item(item);
    }

    /// `titleSort:asc`, as the server wants it.
    fn sort_parameter(&self) -> Option<String> {
        let imp = self.imp();
        let direction = if imp.descending.get() { "desc" } else { "asc" };
        imp.sort
            .borrow()
            .as_ref()
            .map(|sort| format!("{sort}:{direction}"))
    }

    fn reset(&self, title: &str) {
        let imp = self.imp();
        if let Some(task) = imp.task.take() {
            task.abort();
        }
        imp.loading.set(false);
        imp.offset.set(0);
        imp.total.set(0);
        imp._title_label.set_label(title);
        imp._section_flow.clear();
    }

    /// Shows a library, with the sort order used last time.
    pub fn show_section(&self, section: &LibrarySection) {
        let imp = self.imp();
        if imp
            .section
            .borrow()
            .as_ref()
            .is_some_and(|current| current.uuid == section.uuid)
        {
            return;
        }
        self.reset(&section.title);
        imp.section.replace(Some(section.clone()));
        imp._section_controll_box.set_visible(true);

        let (sort, direction) = match store::section_sort(&section.uuid) {
            Some((sort, direction)) => (Some(sort), direction),
            None => (None, "desc".to_owned()),
        };
        imp.sort.replace(sort);
        imp.descending.set(direction != "asc");
        self.update_order_image();

        let Some(server) = Session::current().server() else {
            return;
        };
        let key = section.key;
        imp._load_box.set_visible(true);
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let sorts = runtime::run(async move { server.section_sorts(key).await }).await;
                match sorts {
                    Ok(sorts) => view.set_sorts(sorts.into_iter().map(|sort| sort.key).collect()),
                    Err(err) => report!("Could not load the sort options: {err}"),
                }
                view.load_more();
            }
        ));
        imp.task.replace(Some(task));
    }

    fn set_sorts(&self, keys: Vec<String>) {
        let imp = self.imp();
        let labels: Vec<String> = keys.iter().map(|key| sort_label(key)).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let active = imp
            .sort
            .borrow()
            .as_ref()
            .and_then(|sort| keys.iter().position(|key| key == sort));

        imp.updating_sorts.set(true);
        imp._filter_box
            .set_model(Some(&gtk::StringList::new(&labels)));
        imp._filter_box.set_selected(
            active
                .map(|index| index as u32)
                .unwrap_or(gtk::INVALID_LIST_POSITION),
        );
        imp.updating_sorts.set(false);
        imp.sort_keys.replace(keys);
    }

    fn update_order_image(&self) {
        let imp = self.imp();
        imp._order_image
            .set_icon_name(Some(if imp.descending.get() {
                "go-down-symbolic"
            } else {
                "go-up-symbolic"
            }));
    }

    /// The user picked another sort key or direction: remember and reload.
    fn sort_changed(&self) {
        let imp = self.imp();
        self.update_order_image();
        let Some(section) = imp.section.borrow().clone() else {
            return;
        };
        if let Some(sort) = imp.sort.borrow().as_ref() {
            let direction = if imp.descending.get() { "desc" } else { "asc" };
            store::set_section_sort(&section.uuid, sort, direction);
        }
        self.reset(&section.title);
        self.load_more();
    }

    /// Loads the next page, if there is one and none is being loaded.
    fn load_more(&self) {
        let imp = self.imp();
        let (Some(section), Some(server)) =
            (imp.section.borrow().clone(), Session::current().server())
        else {
            return;
        };
        let offset = imp.offset.get();
        if imp.loading.get() || (offset > 0 && offset >= imp.total.get()) {
            return;
        }
        imp.loading.set(true);
        imp._load_box.set_visible(true);
        let sort = self.sort_parameter();

        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let page = runtime::run(async move {
                    server
                        .section_items(section.key, offset, PAGE_SIZE, sort.as_deref())
                        .await
                })
                .await;
                let imp = view.imp();
                match page {
                    Ok(page) => {
                        imp.total.set(page.total);
                        imp.offset.set(offset + PAGE_SIZE);
                        imp._section_flow.append(page.items);
                    }
                    Err(err) => report!("Could not load the library: {err}"),
                }
                imp.loading.set(false);
                imp._load_box.set_visible(false);
            }
        ));
        imp.task.replace(Some(task));
    }

    pub fn show_playlists(&self) {
        let imp = self.imp();
        self.reset(&gettext("Playlists"));
        imp.section.replace(None);
        imp._section_controll_box.set_visible(false);
        let Some(server) = Session::current().server() else {
            return;
        };
        imp._load_box.set_visible(true);
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let playlists = runtime::run(async move { server.playlists().await }).await;
                let imp = view.imp();
                match playlists {
                    Ok(playlists) => imp._section_flow.set_items(playlists),
                    Err(err) => report!("Could not load the playlists: {err}"),
                }
                imp._load_box.set_visible(false);
            }
        ));
        imp.task.replace(Some(task));
    }
}
