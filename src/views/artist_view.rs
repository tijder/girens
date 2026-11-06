//! An artist as one virtualised list: the artist, then every album with its
//! tracks.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;

use adw::subclass::prelude::*;
use girens_plex::model::Item;
use gtk::prelude::*;
use gtk::{gio, glib, CompositeTemplate};

use super::album_view::{album_count, album_rows, setup_music_list, song_count, MusicRow};
use super::row_list;
use crate::session::Session;
use crate::{images, runtime};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/artist_view.ui")]
    pub struct ArtistView {
        #[template_child]
        pub _list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub _header: TemplateChild<gtk::Box>,
        #[template_child]
        pub _avatar: TemplateChild<adw::Avatar>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _subtitle_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _play_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _shuffle_button: TemplateChild<gtk::Button>,
        pub store: OnceCell<gio::ListStore>,
        pub rating_key: Cell<Option<u64>>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ArtistView {
        const NAME: &'static str = "artist_view";
        type Type = super::ArtistView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ArtistView {
        fn constructed(&self) {
            self.parent_constructed();
            let _ = self.store.set(setup_music_list(&self._list_view));
        }
    }

    impl WidgetImpl for ArtistView {}
    impl BinImpl for ArtistView {}
}

glib::wrapper! {
    pub struct ArtistView(ObjectSubclass<imp::ArtistView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ArtistView {
    /// What is currently displayed.
    pub fn rating_key(&self) -> Option<u64> {
        self.imp().rating_key.get()
    }

    fn header_row(&self) -> MusicRow {
        MusicRow::Widget(self.imp()._header.get().upcast())
    }

    /// Loads and shows the artist with this rating key.
    pub fn show(&self, rating_key: u64) {
        let imp = self.imp();
        imp.rating_key.set(Some(rating_key));
        let store = imp.store.get().expect("set in constructed").clone();
        imp._title_label.set_text("");
        imp._subtitle_label.set_text("");
        imp._avatar.set_text(None);
        imp._avatar.set_custom_image(gtk::gdk::Paintable::NONE);
        row_list::restart(
            &imp._list_view,
            &store,
            self.header_row(),
            vec![MusicRow::Loading],
        );

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
                // Three requests give the whole page: the artist, the albums
                // and all tracks.
                let loaded = runtime::run(async move {
                    tokio::try_join!(
                        server.item(rating_key),
                        server.children(rating_key),
                        server.all_leaves(rating_key)
                    )
                })
                .await;
                let (artist, albums, tracks) = match loaded {
                    Ok(loaded) => loaded,
                    Err(err) => {
                        row_list::replace_from(&store, 1, Vec::<MusicRow>::new());
                        let retry = glib::clone!(
                            #[weak]
                            view,
                            move || view.show(rating_key)
                        );
                        crate::notify::failure_with_retry(
                            &crate::messages::load_artist(),
                            &err,
                            retry,
                        );
                        return;
                    }
                };
                let imp = view.imp();
                imp._title_label.set_text(&artist.title);
                imp._avatar.set_text(Some(&artist.title));
                let genres: Vec<&str> = artist.genres.iter().map(|g| g.tag.as_str()).collect();
                imp._subtitle_label.set_text(&crate::util::detail_line([
                    (!genres.is_empty()).then(|| genres.join(", ")),
                    (!albums.is_empty()).then(|| album_count(albums.len() as u32)),
                    (!tracks.is_empty()).then(|| song_count(tracks.len() as u32)),
                ]));
                // The photo is decoration; the page does not wait for it.
                if let (Some(thumb), Some(server)) =
                    (artist.thumb.clone(), Session::current().server())
                {
                    glib::spawn_future_local(glib::clone!(
                        #[weak]
                        view,
                        async move {
                            let texture = images::cover(&server, rating_key, &thumb).await;
                            // Another artist may be on show by now.
                            if view.rating_key() == Some(rating_key) {
                                view.imp()._avatar.set_custom_image(texture.as_ref());
                            }
                        }
                    ));
                }

                let mut by_album: HashMap<u64, Vec<Item>> = HashMap::new();
                for track in tracks {
                    if let Some(album) = track.parent_rating_key {
                        by_album.entry(album).or_default().push(track);
                    }
                }
                let mut rows = Vec::new();
                for album in &albums {
                    let tracks = by_album.remove(&album.rating_key).unwrap_or_default();
                    rows.extend(album_rows(album, &tracks, false));
                }
                row_list::replace_from(&store, 1, rows);
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }
}
