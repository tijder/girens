//! A cover tile: poster, title, watched badge, progress and context menu.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use girens_plex::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate};

use crate::i18n::gettext;
use crate::images;
use crate::session::Session;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/cover_box.ui")]
    pub struct CoverBox {
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _subtitle_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _progress_bar: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub _watched_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _cover_image: TemplateChild<gtk::Picture>,
        #[template_child]
        pub popover_menu: TemplateChild<gtk::PopoverMenu>,
        #[template_child]
        pub _menu_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _long_press: TemplateChild<gtk::GestureLongPress>,
        #[template_child]
        pub _right_click: TemplateChild<gtk::GestureClick>,
        /// Inside a show page episodes show their own still and no link back.
        pub in_show_view: Cell<bool>,
        pub image_task: RefCell<Option<glib::JoinHandle<()>>>,
        /// The item on show, kept for the context menu.
        pub item: RefCell<Option<Item>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CoverBox {
        const NAME: &'static str = "cover_box";
        type Type = super::CoverBox;
        type ParentType = gtk::Button;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CoverBox {
        fn constructed(&self) {
            self.parent_constructed();
            let cover = self.obj();

            self._long_press.connect_pressed(glib::clone!(
                #[weak]
                cover,
                move |gesture, x, y| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    cover.show_menu(x, y);
                }
            ));
            self._menu_button.connect_clicked(glib::clone!(
                #[weak]
                cover,
                move |_| cover.show_menu(16.0, 16.0)
            ));
            self._right_click.connect_released(glib::clone!(
                #[weak]
                cover,
                move |gesture, _, x, y| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    cover.show_menu(x, y);
                }
            ));
        }

        fn dispose(&self) {
            self.obj().unbind();
        }
    }

    impl WidgetImpl for CoverBox {}
    impl ButtonImpl for CoverBox {}
}

glib::wrapper! {
    pub struct CoverBox(ObjectSubclass<imp::CoverBox>)
        @extends gtk::Button, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for CoverBox {
    fn default() -> Self {
        glib::Object::new()
    }
}

/// What a tile shows for an item of each kind.
struct Presentation {
    title: String,
    subtitle: Option<String>,
    image_height: i32,
    /// `win.` action and its rating key.
    action: Option<(&'static str, u64)>,
    /// Cache key and server path of the image.
    image: Option<(u64, String)>,
}

fn present(item: &Item, in_show_view: bool) -> Presentation {
    let year = || item.year.map(|year| year.to_string());
    let parent = || item.parent_title.clone().unwrap_or_default();
    let own_image = || item.thumb.clone().map(|thumb| (item.rating_key, thumb));

    let (title, subtitle, image_height) = match item.kind {
        ItemKind::Episode => (
            item.grandparent_title.clone().unwrap_or_default(),
            Some(match item.season_episode() {
                Some(number) => format!("{number} - {}", item.title),
                None => item.title.clone(),
            }),
            if in_show_view { 110 } else { 300 },
        ),
        ItemKind::Season => (parent(), Some(item.title.clone()), 300),
        ItemKind::Album => (parent(), Some(item.title.clone()), 200),
        ItemKind::Artist | ItemKind::Playlist => (item.title.clone(), None, 200),
        _ => (item.title.clone(), year(), 300),
    };

    let action = match item.kind {
        ItemKind::Episode | ItemKind::Movie | ItemKind::Playlist | ItemKind::Clip => {
            Some(("win.play-item", item.rating_key))
        }
        ItemKind::Season => item
            .parent_rating_key
            .map(|key| ("win.show-show-by-id", key)),
        ItemKind::Show => Some(("win.show-show-by-id", item.rating_key)),
        ItemKind::Artist => Some(("win.show-artist-by-id", item.rating_key)),
        ItemKind::Album => Some(("win.show-album-by-id", item.rating_key)),
        _ => None,
    };

    let image = match item.kind {
        ItemKind::Playlist => item
            .composite
            .clone()
            .map(|composite| (item.rating_key, composite))
            .or_else(own_image),
        // Outside a show page an episode is shown with the poster of its show.
        ItemKind::Episode if !in_show_view => item
            .grandparent_rating_key
            .zip(item.grandparent_thumb.clone())
            .or_else(own_image),
        _ => own_image(),
    };

    Presentation {
        title,
        subtitle,
        image_height,
        action,
        image,
    }
}

fn menu(item: &Item, in_show_view: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    // Labels are passed translated: xgettext only finds literal
    // `gettext("…")` calls.
    let add = |label: &str, action: &str, key: u64| {
        let entry = gio::MenuItem::new(Some(label), None);
        entry.set_action_and_target_value(Some(action), Some(&(key as i64).to_variant()));
        menu.append_item(&entry);
    };
    let video = matches!(
        item.kind,
        ItemKind::Episode | ItemKind::Season | ItemKind::Show | ItemKind::Movie
    );
    let watched = item.is_watched();
    let started = item.view_offset != 0;

    if item.kind == ItemKind::Album {
        add(
            &gettext("Go to album view"),
            "win.show-album-by-id",
            item.rating_key,
        );
    }
    match item.kind {
        ItemKind::Artist => add(
            &gettext("Go to artist view"),
            "win.show-artist-by-id",
            item.rating_key,
        ),
        ItemKind::Album => {
            if let Some(artist) = item.parent_rating_key {
                add(
                    &gettext("Go to artist view"),
                    "win.show-artist-by-id",
                    artist,
                );
            }
        }
        _ => {}
    }
    if !in_show_view {
        let show = match item.kind {
            ItemKind::Episode => item.grandparent_rating_key,
            ItemKind::Season => item.parent_rating_key,
            ItemKind::Show => Some(item.rating_key),
            _ => None,
        };
        if let Some(show) = show {
            add(&gettext("Go to show view"), "win.show-show-by-id", show);
        }
    }
    if video && (!watched || started) {
        add(
            &gettext("Mark as Played"),
            "win.mark-as-played-by-id",
            item.rating_key,
        );
    }
    if video && (watched || started) {
        add(
            &gettext("Mark as Unplayed"),
            "win.mark-as-unplayed-by-id",
            item.rating_key,
        );
    }
    let offline = Session::current()
        .server()
        .is_some_and(|server| crate::sync::downloaded(&server, item.rating_key).is_some());
    if !offline {
        add(&gettext("Sync"), "win.sync-by-id", item.rating_key);
    }
    if matches!(
        item.kind,
        ItemKind::Album | ItemKind::Artist | ItemKind::Show | ItemKind::Playlist
    ) {
        add(&gettext("Shuffle"), "win.shuffle-by-id", item.rating_key);
    }
    if matches!(item.kind, ItemKind::Episode | ItemKind::Movie) && started {
        add(
            &gettext("Start from the beginning"),
            "win.play-item-from-beginning",
            item.rating_key,
        );
    }
    menu
}

impl CoverBox {
    pub fn new(in_show_view: bool) -> Self {
        let cover = Self::default();
        cover.imp().in_show_view.set(in_show_view);
        cover
    }

    /// Shows `item`. A tile can be reused for another item at any time.
    pub fn set_item(&self, item: &Item, width: i32) {
        let imp = self.imp();
        self.unbind();
        let in_show_view = imp.in_show_view.get();
        let presentation = present(item, in_show_view);

        imp._title_label.set_text(&presentation.title);
        imp._title_label.set_tooltip_text(Some(&presentation.title));
        imp._subtitle_label
            .set_visible(presentation.subtitle.is_some());
        if let Some(subtitle) = &presentation.subtitle {
            imp._subtitle_label.set_text(subtitle);
            imp._subtitle_label.set_tooltip_text(Some(subtitle));
        }

        let playable = matches!(item.kind, ItemKind::Movie | ItemKind::Episode);
        match (playable && item.view_offset != 0, item.duration) {
            (true, Some(duration)) if duration > 0 => {
                imp._progress_bar
                    .set_fraction((item.view_offset as f64 / duration as f64).min(1.0));
                imp._progress_bar.set_visible(true);
            }
            _ => imp._progress_bar.set_visible(false),
        }

        let has_watched_state = !matches!(
            item.kind,
            ItemKind::Playlist | ItemKind::Album | ItemKind::Artist
        );
        imp._watched_image
            .set_visible(has_watched_state && !item.is_watched());

        match presentation.action {
            Some((action, key)) => {
                self.set_action_target_value(Some(&(key as i64).to_variant()));
                self.set_action_name(Some(action));
            }
            None => self.set_action_name(None),
        }
        // The menu is built when it is opened, see `show_menu`.
        imp.item.replace(Some(item.clone()));

        imp._cover_image
            .set_size_request(width, presentation.image_height);
        imp._cover_image.set_paintable(gdk::Paintable::NONE);
        let (Some((key, thumb)), Some(server)) = (presentation.image, Session::current().server())
        else {
            return;
        };
        let task = glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = cover)]
            self,
            async move {
                if let Some(texture) = images::cover(&server, key, &thumb).await {
                    cover.imp()._cover_image.set_paintable(Some(&texture));
                }
            }
        ));
        imp.image_task.replace(Some(task));
    }

    /// Stops loading the image of the current item.
    pub fn unbind(&self) {
        if let Some(task) = self.imp().image_task.take() {
            task.abort();
        }
    }

    fn show_menu(&self, x: f64, y: f64) {
        let imp = self.imp();
        let Some(item) = imp.item.borrow().clone() else {
            return;
        };
        let popover = &imp.popover_menu;
        popover.set_menu_model(Some(&menu(&item, imp.in_show_view.get())));
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 10, 10)));
        popover.popup();
    }
}
