//! A show: its details, then the episodes of the chosen season, as one
//! virtualised list. Episodes are fetched per season, when it is chosen.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;

use adw::subclass::prelude::*;
use girens_plex::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate};

use super::row_list::{self, RowWidget};
use super::ShowViewItem;
use crate::session::Session;
use crate::{images, runtime};

enum Row {
    /// The details of the show; always the first row.
    Header,
    /// Shown while the episodes are fetched.
    Loading,
    Episode(Box<Item>),
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/show_view.ui")]
    pub struct ShowView {
        #[template_child]
        pub _list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub _header: TemplateChild<gtk::Box>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _subtitle_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _description_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _cover_image: TemplateChild<gtk::Picture>,
        #[template_child]
        pub _play_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _shuffle_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _season_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub _season_box: TemplateChild<gtk::Box>,
        pub store: OnceCell<gio::ListStore>,
        pub rating_key: Cell<Option<u64>>,
        /// Episodes of the seasons that were looked at, by rating key of the
        /// season.
        pub episodes: RefCell<HashMap<u64, Vec<Item>>>,
        /// The season whose episodes are shown or being fetched.
        pub season: Cell<Option<u64>>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
        pub season_task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ShowView {
        const NAME: &'static str = "show_view";
        type Type = super::ShowView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ShowView {
        fn constructed(&self) {
            self.parent_constructed();
            let view = self.obj();
            let header = self._header.get();
            let store = row_list::setup(
                &self._list_view,
                move |row: &Row, previous: Option<gtk::Widget>| match row {
                    Row::Header => RowWidget::shared(&header),
                    Row::Loading => row_list::loading_row(),
                    Row::Episode(episode) => match previous.and_downcast::<ShowViewItem>() {
                        Some(item) => {
                            item.set_episode(episode);
                            RowWidget::activatable(&item)
                        }
                        None => RowWidget::activatable(&ShowViewItem::new(episode)),
                    },
                },
                glib::clone!(
                    #[weak]
                    view,
                    move |row: &Row| {
                        if let Row::Episode(episode) = row {
                            let key = (episode.rating_key as i64).to_variant();
                            let _ = view.activate_action("win.play-item", Some(&key));
                        }
                    }
                ),
            );
            let _ = self.store.set(store);
        }
    }

    impl WidgetImpl for ShowView {}
    impl BinImpl for ShowView {}
}

glib::wrapper! {
    pub struct ShowView(ObjectSubclass<imp::ShowView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ShowView {
    /// The show that is currently displayed.
    pub fn rating_key(&self) -> Option<u64> {
        self.imp().rating_key.get()
    }

    fn store(&self) -> &gio::ListStore {
        self.imp().store.get().expect("set in constructed")
    }

    /// Loads and shows the show with this rating key.
    pub fn show(&self, rating_key: u64) {
        let imp = self.imp();
        imp.rating_key.set(Some(rating_key));
        imp._title_label.set_text("");
        imp._subtitle_label.set_text("");
        imp._description_label.set_text("");
        imp._cover_image.set_paintable(gdk::Paintable::NONE);
        imp.episodes.borrow_mut().clear();
        imp.season.set(None);
        if let Some(task) = imp.season_task.take() {
            task.abort();
        }
        while let Some(button) = imp._season_box.first_child() {
            imp._season_box.remove(&button);
        }
        imp._season_scrolled_window.set_visible(false);
        row_list::replace_from(self.store(), 0, vec![Row::Header, Row::Loading]);

        let target = (rating_key as i64).to_variant();
        imp._play_button.set_action_target_value(Some(&target));
        imp._play_button.set_action_name(Some("win.play-item"));
        imp._shuffle_button.set_action_target_value(Some(&target));
        imp._shuffle_button
            .set_action_name(Some("win.shuffle-by-id"));

        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let loaded = {
                    let server = server.clone();
                    runtime::run(async move {
                        tokio::try_join!(server.item(rating_key), server.children(rating_key))
                    })
                    .await
                };
                let (show, children) = match loaded {
                    Ok(loaded) => loaded,
                    Err(err) => {
                        row_list::replace_from(view.store(), 1, Vec::<Row>::new());
                        let retry = glib::clone!(
                            #[weak]
                            view,
                            move || view.show(rating_key)
                        );
                        crate::notify::failure_with_retry(
                            &crate::messages::load_show(),
                            &err,
                            retry,
                        );
                        return;
                    }
                };
                view.fill(&show, children);
                if let Some(thumb) = show.thumb.as_deref() {
                    if let Some(texture) = images::cover(&server, show.rating_key, thumb).await {
                        view.imp()._cover_image.set_paintable(Some(&texture));
                    }
                }
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }

    /// `children` are the seasons of the show, or directly its episodes when
    /// the library does not use seasons.
    fn fill(&self, show: &Item, children: Vec<Item>) {
        let imp = self.imp();
        imp._title_label.set_text(&show.title);
        imp._subtitle_label
            .set_text(&show.year.map(|year| year.to_string()).unwrap_or_default());
        imp._description_label
            .set_text(show.summary.as_deref().unwrap_or_default());

        if children.iter().all(|child| child.kind == ItemKind::Episode) {
            self.show_episodes(&children);
            return;
        }

        // One toggle per season, like tabs.
        let seasons: Vec<&Item> = children
            .iter()
            .filter(|child| child.kind == ItemKind::Season)
            .collect();
        let mut first: Option<gtk::ToggleButton> = None;
        for season in &seasons {
            let button = gtk::ToggleButton::with_label(&season.title);
            match &first {
                Some(first) => button.set_group(Some(first)),
                None => first = Some(button.clone()),
            }
            let key = season.rating_key;
            button.connect_toggled(glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |button| {
                    if button.is_active() {
                        view.show_season(key);
                    }
                }
            ));
            imp._season_box.append(&button);
        }
        imp._season_scrolled_window.set_visible(seasons.len() > 1);
        // Activating the first toggle loads its season.
        if let Some(first) = first {
            first.set_active(true);
        }
    }

    /// Shows a season, fetching its episodes the first time.
    fn show_season(&self, season: u64) {
        let imp = self.imp();
        imp.season.set(Some(season));
        if let Some(task) = imp.season_task.take() {
            task.abort();
        }
        if let Some(episodes) = imp.episodes.borrow().get(&season) {
            self.show_episodes(episodes);
            return;
        }
        row_list::replace_from(self.store(), 1, vec![Row::Loading]);
        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                match runtime::run(async move { server.children(season).await }).await {
                    Ok(episodes) => {
                        let imp = view.imp();
                        if imp.season.get() == Some(season) {
                            view.show_episodes(&episodes);
                        }
                        imp.episodes.borrow_mut().insert(season, episodes);
                    }
                    Err(err) => {
                        row_list::replace_from(view.store(), 1, Vec::<Row>::new());
                        let retry = glib::clone!(
                            #[weak]
                            view,
                            move || view.show_season(season)
                        );
                        crate::notify::failure_with_retry(
                            &crate::messages::load_episodes(),
                            &err,
                            retry,
                        );
                    }
                }
            }
        ));
        imp.season_task.replace(Some(task));
    }

    fn show_episodes(&self, episodes: &[Item]) {
        let rows = episodes
            .iter()
            .map(|episode| Row::Episode(Box::new(episode.clone())))
            .collect();
        // The header stays; only the episodes are exchanged.
        row_list::replace_from(self.store(), 1, rows);
    }
}
