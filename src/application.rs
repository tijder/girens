use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::config;
use crate::window::PlexWindow;

/// Options that can be passed on the command line, also to an already running
/// instance (the GNOME Shell search provider uses `--show-id`).
#[derive(Debug, Default, Clone)]
pub struct LaunchOptions {
    /// `<server machine id>;<rating key>`, split on `;`.
    pub show_id: Option<Vec<String>>,
    /// What to search for, as typed in the GNOME Shell search.
    pub search: Option<String>,
    pub video_output_driver: Option<String>,
    pub deinterlace: Option<String>,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Application;

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "GirensApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {
        fn constructed(&self) {
            self.parent_constructed();
            let app = self.obj();
            app.add_main_option(
                "show-id",
                glib::Char::from(b's'),
                glib::OptionFlags::NONE,
                glib::OptionArg::String,
                "Show id",
                None,
            );
            app.add_main_option(
                "search",
                glib::Char::from(0),
                glib::OptionFlags::NONE,
                glib::OptionArg::String,
                "Search the library",
                None,
            );
            app.add_main_option(
                "video-output-driver",
                glib::Char::from(b'v'),
                // Still accepted, so that launchers made for earlier
                // versions keep working.
                glib::OptionFlags::HIDDEN,
                glib::OptionArg::String,
                "Ignored: Girens renders the video itself",
                None,
            );
            app.add_main_option(
                "deinterlace",
                glib::Char::from(b'd'),
                glib::OptionFlags::NONE,
                glib::OptionArg::String,
                "Use deinterlace",
                None,
            );
        }
    }

    impl ApplicationImpl for Application {
        fn command_line(&self, command_line: &gio::ApplicationCommandLine) -> glib::ExitCode {
            let dict = command_line.options_dict();
            let string_option = |name: &str| dict.lookup::<String>(name).ok().flatten();
            let options = LaunchOptions {
                show_id: string_option("show-id")
                    .map(|value| value.split(';').map(str::to_owned).collect()),
                search: string_option("search").filter(|query| !query.trim().is_empty()),
                video_output_driver: string_option("video-output-driver"),
                deinterlace: string_option("deinterlace"),
            };
            self.obj().present_window(options);
            glib::ExitCode::SUCCESS
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Application {
    pub fn new() -> Self {
        #[allow(unused_mut)]
        let mut flags = gio::ApplicationFlags::HANDLES_COMMAND_LINE;
        // Lets a build under test run next to the installed Girens.
        #[cfg(debug_assertions)]
        if std::env::var_os("GIRENS_DEBUG_NON_UNIQUE").is_some() {
            flags |= gio::ApplicationFlags::NON_UNIQUE;
        }
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("flags", flags)
            .build()
    }

    fn present_window(&self, options: LaunchOptions) {
        let window = match self.active_window().and_downcast::<PlexWindow>() {
            Some(window) => {
                window.apply_launch_options(&options);
                window
            }
            None => PlexWindow::new(self, &options),
        };
        window.present();
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}
