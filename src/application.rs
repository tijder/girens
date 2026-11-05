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
                "video-output-driver",
                glib::Char::from(b'v'),
                glib::OptionFlags::NONE,
                glib::OptionArg::String,
                "Which video output driver mpv will use",
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
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_COMMAND_LINE)
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
