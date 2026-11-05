//! Rating, shuffle and repeat for the music that is playing.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use girens_plex::model::Item;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::player::{Event, Player};
use crate::runtime;
use crate::session::Session;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/music_popover_menu.ui")]
    pub struct MusicPopoverMenu {
        #[template_child]
        pub _star_1_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _star_2_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _star_3_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _star_4_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _star_5_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _star_1_img: TemplateChild<gtk::Image>,
        #[template_child]
        pub _star_2_img: TemplateChild<gtk::Image>,
        #[template_child]
        pub _star_3_img: TemplateChild<gtk::Image>,
        #[template_child]
        pub _star_4_img: TemplateChild<gtk::Image>,
        #[template_child]
        pub _star_5_img: TemplateChild<gtk::Image>,
        #[template_child]
        pub _shuffle_button: TemplateChild<gtk::Switch>,
        #[template_child]
        pub _repeat_button: TemplateChild<gtk::Switch>,
        pub item: RefCell<Option<Item>>,
        /// Set while the switches are updated from the player.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MusicPopoverMenu {
        const NAME: &'static str = "music_popover_menu";
        type Type = super::MusicPopoverMenu;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MusicPopoverMenu {}
    impl WidgetImpl for MusicPopoverMenu {}
    impl PopoverImpl for MusicPopoverMenu {}
}

glib::wrapper! {
    pub struct MusicPopoverMenu(ObjectSubclass<imp::MusicPopoverMenu>)
        @extends gtk::Popover, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
                    gtk::ShortcutManager;
}

impl MusicPopoverMenu {
    pub fn new(player: &Player) -> Self {
        let menu: Self = glib::Object::new();
        let imp = menu.imp();

        let buttons = [
            &imp._star_1_button,
            &imp._star_2_button,
            &imp._star_3_button,
            &imp._star_4_button,
            &imp._star_5_button,
        ];
        for (button, stars) in buttons.into_iter().zip(1u8..) {
            button.connect_clicked(glib::clone!(
                #[weak]
                menu,
                move |_| menu.rate(stars)
            ));
        }

        imp._shuffle_button.connect_active_notify(glib::clone!(
            #[weak]
            menu,
            #[strong]
            player,
            move |switch| {
                if !menu.imp().updating.get() {
                    player.set_shuffle(switch.is_active());
                }
            }
        ));
        imp._repeat_button.connect_active_notify(glib::clone!(
            #[weak]
            menu,
            #[strong]
            player,
            move |switch| {
                if !menu.imp().updating.get() {
                    player.set_repeat(switch.is_active());
                }
            }
        ));

        player.connect(glib::clone!(
            #[weak]
            menu,
            #[strong]
            player,
            move |event| {
                let imp = menu.imp();
                match event {
                    Event::Started { item, video: false } => {
                        imp.item.replace(Some((**item).clone()));
                        let stars = (item.user_rating.unwrap_or(0.0) / 2.0).round() as u8;
                        menu.show_stars(stars);
                        imp.updating.set(true);
                        imp._shuffle_button.set_active(player.is_shuffled());
                        imp._repeat_button.set_active(false);
                        imp.updating.set(false);
                        player.set_repeat(false);
                    }
                    Event::QueueChanged => {
                        imp.updating.set(true);
                        imp._shuffle_button.set_active(player.is_shuffled());
                        imp.updating.set(false);
                    }
                    _ => {}
                }
            }
        ));
        menu
    }

    fn show_stars(&self, stars: u8) {
        let imp = self.imp();
        let images = [
            &imp._star_1_img,
            &imp._star_2_img,
            &imp._star_3_img,
            &imp._star_4_img,
            &imp._star_5_img,
        ];
        for (image, star) in images.into_iter().zip(1u8..) {
            image.set_icon_name(Some(if star <= stars {
                "starred-symbolic"
            } else {
                "non-starred-symbolic"
            }));
        }
    }

    /// Rates the playing track with one to five stars.
    fn rate(&self, stars: u8) {
        self.show_stars(stars);
        let (Some(server), Some(key)) = (
            Session::current().server(),
            self.imp()
                .item
                .borrow()
                .as_ref()
                .map(|item| item.rating_key),
        ) else {
            return;
        };
        glib::spawn_future_local(async move {
            let result =
                runtime::run(async move { server.rate(key, f32::from(stars) * 2.0).await }).await;
            if let Err(err) = result {
                crate::notify::failure(&crate::i18n::gettext("Could not rate the track"), &err);
            }
        });
    }
}
