//! One photo, as large as the window allows, with the others of its album
//! one step away.

use girens_model::ItemId;
use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_model::model::{Item, ItemKind};
use gtk::{gdk, gio, glib};

use crate::i18n::gettext;
use crate::session::Session;
use crate::{runtime, util};

/// The size asked of the server; it scales the photo down to fit.
const WIDTH: u32 = 2560;
const HEIGHT: u32 = 1440;
/// How many photos that are in no album are stepped through at most.
const LOOSE_PHOTOS: u32 = 2000;

mod imp {
    use super::*;

    pub struct PhotoView {
        pub picture: gtk::Picture,
        pub spinner: adw::Spinner,
        pub caption: gtk::Label,
        pub previous: gtk::Button,
        pub next: gtk::Button,
        /// The photos of the album, and which of them is shown.
        pub photos: RefCell<Vec<Item>>,
        pub index: Cell<usize>,
        pub rating_key: Cell<Option<ItemId>>,
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    impl Default for PhotoView {
        fn default() -> Self {
            let step = |icon: &str, tooltip: String, side: gtk::Align| {
                gtk::Button::builder()
                    .icon_name(icon)
                    .tooltip_text(tooltip)
                    .halign(side)
                    .valign(gtk::Align::Center)
                    .margin_start(12)
                    .margin_end(12)
                    .css_classes(["osd", "circular"])
                    .build()
            };
            Self {
                picture: gtk::Picture::builder()
                    .content_fit(gtk::ContentFit::Contain)
                    .can_shrink(true)
                    .hexpand(true)
                    .vexpand(true)
                    .build(),
                spinner: adw::Spinner::builder()
                    .halign(gtk::Align::Center)
                    .valign(gtk::Align::Center)
                    .width_request(48)
                    .height_request(48)
                    .build(),
                caption: gtk::Label::builder()
                    .halign(gtk::Align::Center)
                    .valign(gtk::Align::End)
                    .margin_bottom(12)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .css_classes(["osd", "photo-caption"])
                    .build(),
                previous: step(
                    "go-previous-symbolic",
                    gettext("Previous photo"),
                    gtk::Align::Start,
                ),
                next: step("go-next-symbolic", gettext("Next photo"), gtk::Align::End),
                photos: RefCell::default(),
                index: Cell::default(),
                rating_key: Cell::default(),
                task: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PhotoView {
        const NAME: &'static str = "GirensPhotoView";
        type Type = super::PhotoView;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for PhotoView {
        fn constructed(&self) {
            self.parent_constructed();
            let view = self.obj();
            let overlay = gtk::Overlay::new();
            overlay.set_child(Some(&self.picture));
            overlay.add_overlay(&self.spinner);
            overlay.add_overlay(&self.caption);
            overlay.add_overlay(&self.previous);
            overlay.add_overlay(&self.next);
            view.set_child(Some(&overlay));

            self.previous.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| view.step(-1)
            ));
            self.next.connect_clicked(glib::clone!(
                #[weak]
                view,
                move |_| view.step(1)
            ));
        }
    }

    impl WidgetImpl for PhotoView {}
    impl BinImpl for PhotoView {}
}

glib::wrapper! {
    pub struct PhotoView(ObjectSubclass<imp::PhotoView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for PhotoView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PhotoView {
    /// The photo this page was opened for.
    pub fn rating_key(&self) -> Option<ItemId> {
        self.imp().rating_key.get()
    }

    /// Drops what is shown: it was loaded for someone else.
    pub fn forget(&self) {
        let imp = self.imp();
        if let Some(task) = imp.task.take() {
            task.abort();
        }
        imp.rating_key.set(None);
        imp.photos.borrow_mut().clear();
        imp.picture.set_paintable(gdk::Paintable::NONE);
    }

    /// Shows the photo with this rating key, and finds the other photos of
    /// its album to step through.
    pub fn show(&self, rating_key: ItemId) {
        let imp = self.imp();
        imp.rating_key.set(Some(rating_key));
        imp.photos.borrow_mut().clear();
        imp.index.set(0);
        imp.picture.set_paintable(gdk::Paintable::NONE);
        imp.caption.set_visible(false);
        imp.spinner.set_visible(true);
        self.update_steps();
        let Some(server) = Session::current().server() else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let loaded = runtime::run(async move {
                    let photo = server.item(rating_key).await?;
                    // The photos around it: those of its album, or of its
                    // library when it is in none.
                    let album = match (photo.parent_rating_key, photo.library_section_id) {
                        (Some(album), _) => server.children(album).await?,
                        (None, Some(section)) => {
                            server
                                .section_items(section, 0, LOOSE_PHOTOS, None)
                                .await?
                                .items
                        }
                        (None, None) => Vec::new(),
                    };
                    Ok::<_, girens_model::Error>((photo, album))
                })
                .await;
                let (photo, album) = match loaded {
                    Ok(loaded) => loaded,
                    Err(err) => {
                        view.imp().spinner.set_visible(false);
                        let retry = glib::clone!(
                            #[weak]
                            view,
                            move || view.show(rating_key)
                        );
                        crate::notify::failure_with_retry(
                            &crate::messages::load_photo(),
                            &err,
                            retry,
                        );
                        return;
                    }
                };
                let mut photos: Vec<Item> = album
                    .into_iter()
                    .filter(|item| item.kind == ItemKind::Photo && !item.is_photo_album())
                    .collect();
                let index = photos
                    .iter()
                    .position(|item| item.rating_key == rating_key)
                    .unwrap_or_else(|| {
                        // A photo that is in no album stands alone.
                        photos = vec![photo];
                        0
                    });
                view.imp().photos.replace(photos);
                view.display(index).await;
            }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }

    /// Goes to the photo `offset` places further in the album, if there is
    /// one.
    pub fn step(&self, offset: isize) {
        let imp = self.imp();
        let count = imp.photos.borrow().len();
        let Some(index) = imp
            .index
            .get()
            .checked_add_signed(offset)
            .filter(|index| *index < count)
        else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move { view.display(index).await }
        ));
        if let Some(previous) = imp.task.replace(Some(task)) {
            previous.abort();
        }
    }

    /// Left and right step through the album.
    pub fn handle_key(&self, key: gdk::Key) -> glib::Propagation {
        match key {
            gdk::Key::Left => self.step(-1),
            gdk::Key::Right | gdk::Key::space => self.step(1),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn update_steps(&self) {
        let imp = self.imp();
        let (index, count) = (imp.index.get(), imp.photos.borrow().len());
        imp.previous.set_visible(index > 0);
        imp.next.set_visible(index + 1 < count);
    }

    async fn display(&self, index: usize) {
        let imp = self.imp();
        let (Some(photo), Some(server)) = (
            imp.photos.borrow().get(index).cloned(),
            Session::current().server(),
        ) else {
            return;
        };
        imp.index.set(index);
        self.update_steps();
        let count = imp.photos.borrow().len();
        imp.caption.set_text(&util::detail_line([
            Some(photo.title.clone()),
            (count > 1).then(|| format!("{} / {count}", index + 1)),
        ]));
        imp.caption.set_visible(true);
        imp.spinner.set_visible(true);

        // The picture the server keeps of the photo, scaled by it; the
        // file itself when it keeps none.
        let sources: Vec<String> = photo
            .thumb
            .clone()
            .into_iter()
            .chain(photo.parts().next().map(|part| part.key.clone()))
            .collect();
        let scale = self.scale_factor().clamp(1, 2) as u32;
        let mut texture = Err(gettext("The photo has no picture"));
        for source in sources {
            let url = server.image_url(&source, WIDTH * scale, HEIGHT * scale);
            texture = match runtime::run(async move { girens_model::net::download(&url).await }).await {
                // Decoding a large photo takes a moment; not on the main
                // thread.
                Ok(bytes) => gio::spawn_blocking(move || {
                    gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes))
                        .map_err(|err| err.to_string())
                })
                .await
                .unwrap_or_else(|_| Err("decoding was interrupted".to_owned())),
                Err(err) => Err(err.to_string()),
            };
            if texture.is_ok() {
                break;
            }
        }
        imp.spinner.set_visible(false);
        match texture {
            Ok(texture) => imp.picture.set_paintable(Some(&texture)),
            Err(err) => crate::notify::failure(&crate::messages::load_photo(), &err),
        }
    }
}
