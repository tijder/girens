// Views keep their callbacks in `RefCell<Option<Box<dyn Fn(..)>>>` slots.
#![allow(clippy::type_complexity)]

mod application;
mod config;
#[cfg(debug_assertions)]
mod debug;
mod i18n;
mod images;
mod mpris;
mod notify;
mod player;
mod runtime;
mod search_data;
mod secrets;
mod session;
mod store;
mod sync;
mod util;
mod views;
mod window;

use gettextrs::LocaleCategory;
use gtk::prelude::*;
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    #[cfg(debug_assertions)]
    debug::mark_start();

    // SAFETY: no other thread exists yet, so nothing reads the locale concurrently.
    unsafe {
        gettextrs::setlocale(LocaleCategory::LcAll, "");
        // libmpv refuses to initialise unless LC_NUMERIC is "C".
        gettextrs::setlocale(LocaleCategory::LcNumeric, "C");
    }
    // Keep GTK from resetting LC_NUMERIC when it initialises.
    gtk::disable_setlocale();
    if let Err(err) = gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR) {
        eprintln!("Could not bind the text domain: {err}");
    }
    let _ = gettextrs::bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8");
    let _ = gettextrs::textdomain(config::GETTEXT_PACKAGE);

    let resource_path = format!("{}/girens.gresource", config::PKGDATADIR);
    match gio::Resource::load(&resource_path) {
        Ok(resource) => gio::resources_register(&resource),
        Err(err) => {
            eprintln!("Could not load {resource_path}: {err}");
            return glib::ExitCode::FAILURE;
        }
    }

    glib::set_application_name("Girens");
    glib::set_prgname(Some("girens"));

    application::Application::new().run()
}
