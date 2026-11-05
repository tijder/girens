//! System / light / dark chooser in the main menu (after Dialect's).

use std::cell::Cell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib, CompositeTemplate};

use crate::config;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, glib::Properties)]
    #[template(resource = "/nl/g4d/Girens/theme_switcher.ui")]
    #[properties(wrapper_type = super::ThemeSwitcher)]
    pub struct ThemeSwitcher {
        #[template_child]
        pub system: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub light: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub dark: TemplateChild<gtk::CheckButton>,
        /// Whether the platform has a light/dark preference to follow.
        #[property(get, set)]
        pub show_system: Cell<bool>,
        pub settings: gio::Settings,
    }

    impl Default for ThemeSwitcher {
        fn default() -> Self {
            Self {
                system: TemplateChild::default(),
                light: TemplateChild::default(),
                dark: TemplateChild::default(),
                show_system: Cell::new(true),
                settings: gio::Settings::new(config::APP_ID),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ThemeSwitcher {
        const NAME: &'static str = "theme_switcher";
        type Type = super::ThemeSwitcher;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[gtk::template_callbacks]
    impl ThemeSwitcher {
        #[template_callback(name = "_on_color_scheme_changed")]
        fn on_color_scheme_changed(&self) {
            let scheme = if self.dark.is_active() {
                "dark"
            } else if self.light.is_active() {
                "light"
            } else if self.system.is_active() {
                "auto"
            } else {
                // Between unchecking one button and checking the next.
                return;
            };
            if self.settings.string("color-scheme") != scheme {
                if let Err(err) = self.settings.set_string("color-scheme", scheme) {
                    eprintln!("Could not save the color scheme: {err}");
                }
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ThemeSwitcher {
        fn constructed(&self) {
            self.parent_constructed();
            let switcher = self.obj();

            let provider = gtk::CssProvider::new();
            provider.load_from_resource(&format!("{}/theme_switcher.css", config::RESOURCE_PREFIX));
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }

            adw::StyleManager::default()
                .bind_property("system-supports-color-schemes", &*switcher, "show-system")
                .sync_create()
                .build();

            self.settings.connect_changed(
                Some("color-scheme"),
                glib::clone!(
                    #[weak]
                    switcher,
                    move |_, _| switcher.apply()
                ),
            );
            switcher.apply();
        }
    }

    impl WidgetImpl for ThemeSwitcher {}
    impl BoxImpl for ThemeSwitcher {}
}

glib::wrapper! {
    pub struct ThemeSwitcher(ObjectSubclass<imp::ThemeSwitcher>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ThemeSwitcher {
    /// Shows the stored color scheme and applies it to the application.
    fn apply(&self) {
        let imp = self.imp();
        let (button, scheme) = match imp.settings.string("color-scheme").as_str() {
            "light" => (&imp.light, adw::ColorScheme::ForceLight),
            "dark" => (&imp.dark, adw::ColorScheme::ForceDark),
            _ => (&imp.system, adw::ColorScheme::Default),
        };
        button.set_active(true);
        adw::StyleManager::default().set_color_scheme(scheme);
    }
}
