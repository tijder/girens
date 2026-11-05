//! An album as one virtualised list: its header, then the tracks. The rows
//! are shared with the artist page, which lists several albums.

use std::cell::{OnceCell, RefCell};

use adw::subclass::prelude::*;
use girens_plex::model::Item;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate};

use super::row_list::{self, RowWidget};
use super::AlbumItem;
use crate::i18n::{gettext, gettext_f};
use crate::session::Session;
use crate::{images, runtime};

/// A row of an album or artist page.
pub enum MusicRow {
    /// The header of the artist page.
    Widget(gtk::Widget),
    Album {
        album: Box<Item>,
        /// Whether to offer a link to the artist (not on the artist's page).
        link_to_artist: bool,
    },
    Disc(u32),
    Track(Box<Item>),
}

/// The rows for `album`: header, and tracks grouped per disc when there are
/// several discs.
pub fn album_rows(album: &Item, tracks: &[Item], link_to_artist: bool) -> Vec<MusicRow> {
    let mut tracks: Vec<&Item> = tracks.iter().collect();
    tracks.sort_by_key(|track| (track.parent_index.unwrap_or(1), track.index.unwrap_or(0)));
    let several_discs = tracks
        .iter()
        .any(|track| track.parent_index.unwrap_or(1) != 1);

    let mut rows = vec![MusicRow::Album {
        album: Box::new(album.clone()),
        link_to_artist,
    }];
    let mut current_disc = None;
    for track in tracks {
        let disc = track.parent_index.unwrap_or(1);
        if several_discs && current_disc != Some(disc) {
            current_disc = Some(disc);
            rows.push(MusicRow::Disc(disc));
        }
        rows.push(MusicRow::Track(Box::new(track.clone())));
    }
    rows
}

/// Makes `list` show [`MusicRow`]s and returns its store.
pub fn setup_music_list(list: &gtk::ListView) -> gio::ListStore {
    row_list::setup(
        list,
        |row: &MusicRow, previous: Option<gtk::Widget>| match row {
            MusicRow::Widget(widget) => RowWidget::shared(widget),
            MusicRow::Album {
                album,
                link_to_artist,
            } => RowWidget::inert(&AlbumHeader::new(album, *link_to_artist)),
            MusicRow::Disc(disc) => {
                let label = gtk::Label::new(Some(&gettext_f("Disc {}", &[&disc.to_string()])));
                label.add_css_class("heading");
                label.set_margin_top(10);
                label.set_margin_bottom(6);
                RowWidget::inert(&label)
            }
            MusicRow::Track(track) => match previous.and_downcast::<AlbumItem>() {
                Some(item) => {
                    item.set_track(track);
                    RowWidget::activatable(&item)
                }
                None => RowWidget::activatable(&AlbumItem::new(track)),
            },
        },
        glib::clone!(
            #[weak]
            list,
            move |row: &MusicRow| {
                if let MusicRow::Track(track) = row {
                    let key = (track.rating_key as i64).to_variant();
                    let _ = list.activate_action("win.play-item", Some(&key));
                }
            }
        ),
    )
}

// The header of one album

mod header {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/album_header.ui")]
    pub struct AlbumHeader {
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _subtitle_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _cover_picture: TemplateChild<gtk::Image>,
        #[template_child]
        pub _cover_placeholder: TemplateChild<gtk::Image>,
        #[template_child]
        pub _menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub _play_button: TemplateChild<gtk::Button>,
        pub cover_task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumHeader {
        const NAME: &'static str = "album_header";
        type Type = super::AlbumHeader;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AlbumHeader {
        fn dispose(&self) {
            if let Some(task) = self.cover_task.take() {
                task.abort();
            }
        }
    }

    impl WidgetImpl for AlbumHeader {}
    impl BoxImpl for AlbumHeader {}
}

glib::wrapper! {
    pub struct AlbumHeader(ObjectSubclass<header::AlbumHeader>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl AlbumHeader {
    fn new(album: &Item, link_to_artist: bool) -> Self {
        let header: Self = glib::Object::new();
        let imp = header.imp();

        imp._title_label.set_text(&album.title);
        imp._subtitle_label
            .set_text(&album.year.map(|year| year.to_string()).unwrap_or_default());
        imp._play_button
            .set_action_target_value(Some(&(album.rating_key as i64).to_variant()));
        imp._play_button.set_action_name(Some("win.play-item"));

        let menu = gio::Menu::new();
        let add = |label: &str, action: &str, key: u64| {
            let entry = gio::MenuItem::new(Some(&gettext(label)), None);
            entry.set_action_and_target_value(Some(action), Some(&(key as i64).to_variant()));
            menu.append_item(&entry);
        };
        if let (true, Some(artist)) = (link_to_artist, album.parent_rating_key) {
            add("Go to artist view", "win.show-artist-by-id", artist);
        }
        add("Sync", "win.sync-by-id", album.rating_key);
        add("Shuffle", "win.shuffle-by-id", album.rating_key);
        imp._menu_button.set_menu_model(Some(&menu));

        imp._cover_picture.set_paintable(gdk::Paintable::NONE);
        if let (Some(thumb), Some(server)) = (album.thumb.clone(), Session::current().server()) {
            let key = album.rating_key;
            let task = glib::spawn_future_local(glib::clone!(
                #[weak]
                header,
                async move {
                    if let Some(texture) = images::cover(&server, key, &thumb).await {
                        let imp = header.imp();
                        imp._cover_picture.set_paintable(Some(&texture));
                        imp._cover_placeholder.set_visible(false);
                    }
                }
            ));
            imp.cover_task.replace(Some(task));
        }
        header
    }
}

// The album page

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/album_view.ui")]
    pub struct AlbumView {
        #[template_child]
        pub _list_view: TemplateChild<gtk::ListView>,
        pub store: OnceCell<gio::ListStore>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumView {
        const NAME: &'static str = "album_view";
        type Type = super::AlbumView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AlbumView {
        fn constructed(&self) {
            self.parent_constructed();
            let _ = self.store.set(setup_music_list(&self._list_view));
        }
    }

    impl WidgetImpl for AlbumView {}
    impl BinImpl for AlbumView {}
}

glib::wrapper! {
    pub struct AlbumView(ObjectSubclass<imp::AlbumView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AlbumView {
    /// Loads and shows the album with this rating key.
    pub fn show(&self, rating_key: u64) {
        let imp = self.imp();
        let store = imp.store.get().expect("set in constructed").clone();
        store.remove_all();
        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(async move {
            let loaded = runtime::run(async move {
                tokio::try_join!(server.item(rating_key), server.children(rating_key))
            })
            .await;
            match loaded {
                Ok((album, tracks)) => {
                    row_list::replace_from(&store, 0, album_rows(&album, &tracks, true))
                }
                Err(err) => {
                    crate::notify::failure(&crate::i18n::gettext("Could not load the album"), &err)
                }
            }
        });
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }
}
