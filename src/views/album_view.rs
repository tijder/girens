//! An album or playlist as one virtualised list: its header, then the
//! tracks. The rows are shared with the artist page, which lists several
//! albums.

use girens_model::ItemId;
use std::cell::{Cell, OnceCell, RefCell};

use adw::subclass::prelude::*;
use girens_model::model::Item;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate};

use super::row_list::{self, RowWidget};
use super::AlbumItem;
use crate::i18n::{gettext, gettext_f, ngettext_f};
use crate::session::Session;
use crate::{images, runtime, util};

/// A row of an album or artist page.
pub enum MusicRow {
    /// The header of the artist page.
    Widget(gtk::Widget),
    Album {
        album: Box<Item>,
        /// Whether this is the page of the album itself, which links to the
        /// artist; the page of an artist lists several albums.
        link_to_artist: bool,
        tracks: u32,
        /// Playing time of all tracks, in milliseconds.
        duration: u64,
    },
    Disc(u32),
    Track(Box<Item>),
    /// The header of the page of a playlist.
    Playlist {
        playlist: Box<Item>,
        items: u32,
        /// Playing time of everything in it, in milliseconds.
        duration: u64,
    },
    /// An item of the playlist with this rating key; playing it continues
    /// with what follows it there.
    Entry {
        playlist: ItemId,
        item: Box<Item>,
    },
    /// Shown while the page is fetched.
    Loading,
}

/// `12 songs`.
pub fn song_count(count: u32) -> String {
    ngettext_f("{} song", "{} songs", count)
}

/// `3 albums`.
pub fn album_count(count: u32) -> String {
    ngettext_f("{} album", "{} albums", count)
}

/// `40 items`, for a playlist: it may hold videos as well as music.
fn item_count(count: u32) -> String {
    ngettext_f("{} item", "{} items", count)
}

/// A playing time in words: `52 min`, `1 h 4 min`.
fn playing_time(milliseconds: u64) -> String {
    let minutes = (milliseconds / 60_000).max(1);
    if minutes < 60 {
        gettext_f("{} min", &[&minutes.to_string()])
    } else {
        gettext_f(
            "{0} h {1} min",
            &[&(minutes / 60).to_string(), &(minutes % 60).to_string()],
        )
    }
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
        tracks: tracks.len() as u32,
        duration: tracks.iter().filter_map(|track| track.duration).sum(),
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

/// How many items of a playlist are fetched at a time.
const PLAYLIST_PAGE: u32 = 200;

/// The rows for items of the playlist `playlist`, the first of which is at
/// `offset` in it.
fn playlist_rows(playlist: ItemId, items: &[Item], offset: u32) -> Vec<MusicRow> {
    let rows = items.iter().enumerate().map(|(position, item)| {
        // A row shows a number and a title: here the place in the playlist,
        // and whose track or which episode it is.
        let mut shown = item.clone();
        shown.index = Some(offset + position as u32 + 1);
        shown.title = util::detail_line([
            Some(item.title.clone()),
            item.grandparent_title.clone(),
            item.season_episode(),
        ]);
        MusicRow::Entry {
            playlist,
            item: Box::new(shown),
        }
    });
    rows.collect()
}

/// Makes `list` show [`MusicRow`]s and returns its store.
pub fn setup_music_list(list: &gtk::ListView) -> gio::ListStore {
    let store = music_store(list);
    // Rebinds the visible rows so that the mark of the playing track moves.
    crate::player::connect_now_playing(glib::clone!(
        #[weak]
        store,
        move || {
            let count = store.n_items();
            store.items_changed(0, count, count);
        }
    ));
    store
}

fn music_store(list: &gtk::ListView) -> gio::ListStore {
    row_list::setup(
        list,
        |row: &MusicRow, previous: Option<gtk::Widget>| match row {
            MusicRow::Widget(widget) => RowWidget::shared(widget),
            MusicRow::Loading => row_list::loading_row(),
            MusicRow::Album {
                album,
                link_to_artist,
                tracks,
                duration,
            } => RowWidget::inert(&AlbumHeader::new(
                album,
                *link_to_artist,
                *tracks,
                *duration,
            )),
            MusicRow::Disc(disc) => {
                let label = gtk::Label::new(Some(&gettext_f("Disc {}", &[&disc.to_string()])));
                label.add_css_class("heading");
                label.set_margin_top(10);
                label.set_margin_bottom(6);
                RowWidget::inert(&label)
            }
            MusicRow::Playlist {
                playlist,
                items,
                duration,
            } => RowWidget::inert(&AlbumHeader::for_playlist(playlist, *items, *duration)),
            MusicRow::Track(track) | MusicRow::Entry { item: track, .. } => match previous
                .and_downcast::<AlbumItem>()
            {
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
            move |row: &MusicRow| match row {
                MusicRow::Track(track) => {
                    let key = track.rating_key.as_str().to_variant();
                    let _ = list.activate_action("win.play-item", Some(&key));
                }
                MusicRow::Entry { playlist, item } => {
                    let keys = (playlist.as_str(), item.rating_key.as_str()).to_variant();
                    let _ = list.activate_action("win.play-playlist-item", Some(&keys));
                }
                _ => {}
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
        pub _artist_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _shuffle_button: TemplateChild<gtk::Button>,
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
    fn new(album: &Item, link_to_artist: bool, tracks: u32, duration: u64) -> Self {
        let header: Self = glib::Object::new();
        let imp = header.imp();

        header.describe(
            album,
            &util::detail_line([
                album.year.map(|year| year.to_string()),
                (tracks > 0).then(|| song_count(tracks)),
                (duration > 0).then(|| playing_time(duration)),
            ]),
        );

        // On its own page the album is what the page is about: its artist is
        // one click away and playing it is the obvious thing to do.
        if link_to_artist {
            imp._play_button.add_css_class("suggested-action");
            if let (Some(artist), Some(name)) = (album.parent_rating_key, &album.parent_title) {
                imp._artist_label.set_markup(&format!(
                    "<a href=\"artist\">{}</a>",
                    glib::markup_escape_text(name)
                ));
                imp._artist_label.set_visible(true);
                imp._artist_label.connect_activate_link(move |label, _| {
                    let key = artist.as_str().to_variant();
                    let _ = label.activate_action("win.show-artist-by-id", Some(&key));
                    glib::Propagation::Stop
                });
            }
        }

        let menu = gio::Menu::new();
        let add = |label: &str, action: &str, key: ItemId| {
            let entry = gio::MenuItem::new(Some(label), None);
            entry.set_action_and_target_value(Some(action), Some(&key.as_str().to_variant()));
            menu.append_item(&entry);
        };
        if let (true, Some(artist)) = (link_to_artist, album.parent_rating_key) {
            add(
                &gettext("Go to artist view"),
                "win.show-artist-by-id",
                artist,
            );
        }
        if crate::session::Session::current().can_download() {
            add(&gettext("Sync"), "win.sync-by-id", album.rating_key);
        }
        imp._menu_button.set_menu_model(Some(&menu));
        imp._menu_button.set_visible(menu.n_items() > 0);

        header.load_cover(album.rating_key, album.thumb.clone());
        header
    }

    /// The header of the page of a playlist.
    fn for_playlist(playlist: &Item, items: u32, duration: u64) -> Self {
        let header: Self = glib::Object::new();
        let imp = header.imp();
        header.describe(
            playlist,
            &util::detail_line([
                (items > 0).then(|| item_count(items)),
                (duration > 0).then(|| playing_time(duration)),
            ]),
        );
        imp._play_button.add_css_class("suggested-action");

        let menu = gio::Menu::new();
        if crate::session::Session::current().can_download() {
            let entry = gio::MenuItem::new(Some(&gettext("Sync")), None);
            let key = playlist.rating_key.as_str().to_variant();
            entry.set_action_and_target_value(Some("win.sync-by-id"), Some(&key));
            menu.append_item(&entry);
        }
        imp._menu_button.set_menu_model(Some(&menu));
        imp._menu_button.set_visible(menu.n_items() > 0);

        // A playlist is pictured by the covers of what is in it.
        let image = playlist.composite.clone().or_else(|| playlist.thumb.clone());
        header.load_cover(playlist.rating_key, image);
        header
    }

    /// Fills in the title, the line below it and what the buttons play.
    fn describe(&self, item: &Item, details: &str) {
        let imp = self.imp();
        imp._title_label.set_text(&item.title);
        imp._subtitle_label.set_text(details);
        let target = item.rating_key.as_str().to_variant();
        imp._play_button.set_action_target_value(Some(&target));
        imp._play_button.set_action_name(Some("win.play-item"));
        imp._shuffle_button.set_action_target_value(Some(&target));
        imp._shuffle_button
            .set_action_name(Some("win.shuffle-by-id"));
    }

    fn load_cover(&self, key: ItemId, thumb: Option<String>) {
        let header = self;
        let imp = self.imp();
        imp._cover_picture.set_paintable(gdk::Paintable::NONE);
        if let (Some(thumb), Some(server)) = (thumb, Session::current().server()) {
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
        pub rating_key: Cell<Option<ItemId>>,
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
    /// What is currently displayed.
    pub fn rating_key(&self) -> Option<ItemId> {
        self.imp().rating_key.get()
    }

    /// Loads and shows the playlist with this rating key. Its items come
    /// in a page at a time, as a playlist can be very long.
    pub fn show_playlist(&self, rating_key: ItemId) {
        let imp = self.imp();
        imp.rating_key.set(Some(rating_key));
        let store = imp.store.get().expect("set in constructed").clone();
        row_list::replace_from(&store, 0, vec![MusicRow::Loading]);
        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let failed = |err: girens_model::Error| {
                    let retry = glib::clone!(
                        #[weak]
                        view,
                        move || view.show_playlist(rating_key)
                    );
                    crate::notify::failure_with_retry(
                        &crate::messages::load_playlist(),
                        &err,
                        retry,
                    );
                };
                let page = |offset: u32| {
                    let server = server.clone();
                    runtime::run(async move {
                        server
                            .playlist_page(rating_key, offset, PLAYLIST_PAGE)
                            .await
                    })
                };

                let loaded = {
                    let server = server.clone();
                    let (playlist, first) = tokio::join!(
                        runtime::run(async move { server.item(rating_key).await }),
                        page(0)
                    );
                    playlist.and_then(|playlist| Ok((playlist, first?)))
                };
                let (playlist, first) = match loaded {
                    Ok(loaded) => loaded,
                    Err(err) => {
                        row_list::replace_from(&store, 0, Vec::<MusicRow>::new());
                        failed(err);
                        return;
                    }
                };
                let total = first.total;
                let mut rows = vec![MusicRow::Playlist {
                    items: playlist.leaf_count.unwrap_or(total),
                    duration: playlist.duration.unwrap_or(0),
                    playlist: Box::new(playlist),
                }];
                rows.extend(playlist_rows(rating_key, &first.items, 0));
                let mut loaded = first.items.len() as u32;
                row_list::replace_from(&store, 0, rows);

                // The rest follows while the start can be used already.
                while loaded < total {
                    match page(loaded).await {
                        Ok(next) if next.items.is_empty() => break,
                        Ok(next) => {
                            let rows = playlist_rows(rating_key, &next.items, loaded);
                            loaded += next.items.len() as u32;
                            row_list::replace_from(&store, store.n_items(), rows);
                        }
                        Err(err) => {
                            failed(err);
                            break;
                        }
                    }
                }
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }

    /// Loads and shows the album with this rating key.
    pub fn show(&self, rating_key: ItemId) {
        let imp = self.imp();
        imp.rating_key.set(Some(rating_key));
        let store = imp.store.get().expect("set in constructed").clone();
        row_list::replace_from(&store, 0, vec![MusicRow::Loading]);
        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let loaded = runtime::run(async move {
                    tokio::try_join!(server.item(rating_key), server.children(rating_key))
                })
                .await;
                match loaded {
                    Ok((album, tracks)) => {
                        row_list::replace_from(&store, 0, album_rows(&album, &tracks, true))
                    }
                    Err(err) => {
                        row_list::replace_from(&store, 0, Vec::<MusicRow>::new());
                        let retry = glib::clone!(
                            #[weak]
                            view,
                            move || view.show(rating_key)
                        );
                        crate::notify::failure_with_retry(
                            &crate::messages::load_album(),
                            &err,
                            retry,
                        );
                    }
                }
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }
}
