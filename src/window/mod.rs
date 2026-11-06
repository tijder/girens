use girens_model::ItemId;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use girens_plex::myplex::Resource;
use girens_model::Error;
use gtk::{gdk, gio, glib, CompositeTemplate};

use crate::application::{Application, LaunchOptions};
use crate::config;
use crate::i18n::{gettext, gettext_f};
use crate::player::{Event, PlayTarget, Player, StartMode};
use crate::runtime;
use crate::session::Session;
use crate::store::SyncEntry;
use crate::sync::{self, Sync};
use crate::views::{
    self, Destination, DownloadMenu, LoginRequest, MediaControls, ProfileDialog, SyncDialog,
    SyncSettings,
};

/// Pause in typing after which a search starts.
const SEARCH_DELAY: std::time::Duration = std::time::Duration::from_millis(300);
/// Seconds between attempts to reach a server that went away.
const RECONNECT_INTERVAL: u32 = 10;
/// Pause in resizing after which the layout adapts to the new width.
const LAYOUT_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

mod connection;
mod navigation;
mod playback;

use navigation::Location;

mod imp {
    use super::*;

    #[derive(CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/main_window.ui")]
    pub struct PlexWindow {
        #[template_child(id = "_viewStack_frame")]
        pub frame_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub _login_view: TemplateChild<views::LoginView>,
        #[template_child]
        pub _loading_view: TemplateChild<views::LoadingView>,
        #[template_child]
        pub _split_view: TemplateChild<adw::OverlaySplitView>,
        #[template_child]
        pub _sidebar_box: TemplateChild<views::SidebarBox>,
        #[template_child]
        pub _profile_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _back_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _avatar_image: TemplateChild<gtk::Image>,

        #[template_child(id = "_viewStack_pages")]
        pub page_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub _discover_view: TemplateChild<views::DiscoverView>,
        #[template_child]
        pub _section_view: TemplateChild<views::SectionView>,
        #[template_child]
        pub _search_view: TemplateChild<views::SearchView>,
        #[template_child]
        pub _show_view: TemplateChild<views::ShowView>,
        #[template_child]
        pub _artist_view: TemplateChild<views::ArtistView>,
        #[template_child]
        pub _album_view: TemplateChild<views::AlbumView>,
        #[template_child]
        pub _player_view: TemplateChild<views::PlayerView>,
        #[template_child]
        pub _media_box_music: TemplateChild<views::MediaBoxMusic>,
        #[template_child]
        pub content_header: TemplateChild<adw::HeaderBar>,

        #[template_child]
        pub _search_toggle_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub _search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub _search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub _sync_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _sync_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub _download_button: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub _menu_popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub _about_button: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _shortcuts_button: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub _preferences_button: TemplateChild<adw::ActionRow>,

        pub settings: gio::Settings,
        pub session: Rc<Session>,
        pub launch_options: RefCell<LaunchOptions>,
        /// The servers offered in the sidebar, in the same order.
        pub servers: RefCell<Vec<Resource>>,
        pub player: OnceCell<Player>,
        pub sync_dialog: OnceCell<SyncDialog>,
        /// The dialog with accounts and profiles, while it is open.
        pub accounts_dialog: glib::WeakRef<ProfileDialog>,
        pub downloads_view: OnceCell<views::DownloadsView>,
        /// The page of an album of photos, and that of one photo.
        pub photos_view: OnceCell<views::SectionView>,
        pub photo_view: OnceCell<views::PhotoView>,
        pub music_controls: OnceCell<MediaControls>,
        /// Cookie of the idle inhibitor that is held while video plays.
        pub inhibit_cookie: Cell<Option<u32>>,
        /// Whether a video is loaded, and whether it is paused.
        pub video_playing: Cell<bool>,
        pub video_paused: Cell<bool>,
        /// Set once playback was ended for closing the window.
        pub closing: Cell<bool>,
        pub search_timeout: RefCell<Option<glib::SourceId>>,
        /// The pages the user came through; the last one is on screen.
        pub history: RefCell<Vec<Location>>,
        /// Set while the connection to the server is being checked or restored.
        pub checking_connection: Cell<bool>,
        pub layout_timeout: RefCell<Option<glib::SourceId>>,
    }

    impl Default for PlexWindow {
        fn default() -> Self {
            let settings = gio::Settings::new(config::APP_ID);
            Self {
                frame_stack: TemplateChild::default(),
                _login_view: TemplateChild::default(),
                _loading_view: TemplateChild::default(),
                _split_view: TemplateChild::default(),
                _sidebar_box: TemplateChild::default(),
                _profile_button: TemplateChild::default(),
                _back_button: TemplateChild::default(),
                _avatar_image: TemplateChild::default(),
                page_stack: TemplateChild::default(),
                _discover_view: TemplateChild::default(),
                _section_view: TemplateChild::default(),
                _search_view: TemplateChild::default(),
                _show_view: TemplateChild::default(),
                _artist_view: TemplateChild::default(),
                _album_view: TemplateChild::default(),
                _player_view: TemplateChild::default(),
                _media_box_music: TemplateChild::default(),
                content_header: TemplateChild::default(),
                _search_toggle_button: TemplateChild::default(),
                _search_bar: TemplateChild::default(),
                _search_entry: TemplateChild::default(),
                _sync_button: TemplateChild::default(),
                _sync_image: TemplateChild::default(),
                _download_button: TemplateChild::default(),
                _menu_popover: TemplateChild::default(),
                _about_button: TemplateChild::default(),
                _shortcuts_button: TemplateChild::default(),
                _preferences_button: TemplateChild::default(),
                session: Session::current(),
                settings,
                launch_options: RefCell::default(),
                servers: RefCell::default(),
                player: OnceCell::new(),
                sync_dialog: OnceCell::new(),
                accounts_dialog: glib::WeakRef::new(),
                downloads_view: OnceCell::new(),
                photos_view: OnceCell::new(),
                photo_view: OnceCell::new(),
                music_controls: OnceCell::new(),
                inhibit_cookie: Cell::new(None),
                video_playing: Cell::new(false),
                video_paused: Cell::new(false),
                closing: Cell::new(false),
                search_timeout: RefCell::new(None),
                history: RefCell::default(),
                checking_connection: Cell::new(false),
                layout_timeout: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlexWindow {
        const NAME: &'static str = "PlexWindow";
        type Type = super::PlexWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            views::ensure_types();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlexWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let window = self.obj();

            if config::BUILD_TYPE == "debug" {
                window.add_css_class("devel");
            }
            window.load_css();
            window.setup_toasts();
            window.restore_size();
            window.setup_callbacks();
            window.setup_player();
            window.setup_actions();
            window.setup_layout();
            window.setup_navigation();
            window.setup_shortcuts();

            #[cfg(debug_assertions)]
            crate::debug::schedule_screenshot(window.upcast_ref());

            glib::spawn_future_local(glib::clone!(
                #[weak]
                window,
                async move { window.start().await }
            ));
        }
    }

    impl WidgetImpl for PlexWindow {}

    impl WindowImpl for PlexWindow {
        fn close_request(&self) -> glib::Propagation {
            let window = self.obj();
            window.save_size();
            let playing = self.player.get().filter(|player| player.is_playing());
            if let (Some(player), false) = (playing, self.closing.get()) {
                // Tell the server where playback stopped before going away.
                self.closing.set(true);
                window.set_visible(false);
                let player = player.clone();
                glib::spawn_future_local(glib::clone!(
                    #[weak]
                    window,
                    async move {
                        player.shutdown().await;
                        window.close();
                    }
                ));
                return glib::Propagation::Stop;
            }
            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for PlexWindow {}
    impl AdwApplicationWindowImpl for PlexWindow {}
}

glib::wrapper! {
    pub struct PlexWindow(ObjectSubclass<imp::PlexWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl PlexWindow {
    pub fn new(application: &Application, options: &LaunchOptions) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", application)
            .build();
        window.apply_launch_options(options);
        window
    }

    /// Merges options from a (possibly second) command-line invocation.
    pub fn apply_launch_options(&self, options: &LaunchOptions) {
        let mut current = self.imp().launch_options.borrow_mut();
        if options.show_id.is_some() {
            current.show_id.clone_from(&options.show_id);
        }
        if options.search.is_some() {
            current.search.clone_from(&options.search);
        }
        if options.video_output_driver.is_some() {
            current
                .video_output_driver
                .clone_from(&options.video_output_driver);
        }
        if options.deinterlace.is_some() {
            current.deinterlace.clone_from(&options.deinterlace);
        }
        if let (Some(mode), Some(player)) = (&options.deinterlace, self.imp().player.get()) {
            player.set_deinterlace(mode);
        }
        if options.video_output_driver.is_some() {
            // Video is drawn inside the window through libmpv's render API,
            // so there is no output driver to choose any more.
            eprintln!("--video-output-driver is ignored: Girens renders the video itself");
        }
        drop(current);
        // A running, connected instance opens the requested item right away.
        if self.imp().session.server().is_some() {
            self.open_requested_item();
        }
    }

    /// Opens the item given with `--show-id <server>;<rating key>`, as the
    /// search provider does for a search result, or the results for
    /// `--search <text>`.
    fn open_requested_item(&self) {
        let search = self.imp().launch_options.borrow_mut().search.take();
        if let Some(query) = search {
            self.search(&query);
        }
        let Some(request) = self.imp().launch_options.borrow_mut().show_id.take() else {
            return;
        };
        let (Some(key), Some(server)) = (
            request.get(1).map(|key| ItemId::new(key)),
            self.imp().session.server(),
        ) else {
            return;
        };
        // The item may be on another server than the one in use.
        let machine = request.first().filter(|machine| !machine.is_empty());
        if let Some(machine) = machine.filter(|machine| *machine != server.machine_identifier()) {
            let resource = self
                .imp()
                .servers
                .borrow()
                .iter()
                .find(|resource| resource.client_identifier == *machine)
                .cloned();
            if self.imp().session.account().is_some() && self.imp().servers.borrow().is_empty() {
                // plex.tv has not listed the servers yet; ask again then.
                self.imp().launch_options.borrow_mut().show_id = Some(request);
                return;
            }
            let Some(resource) = resource else {
                crate::notify::message(&gettext("This item is on a server that is not in use"));
                return;
            };
            self.imp().launch_options.borrow_mut().show_id = Some(request);
            glib::spawn_future_local(glib::clone!(
                #[weak(rename_to = window)]
                self,
                async move { window.use_server(&resource).await }
            ));
            return;
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                use girens_model::model::ItemKind;
                let action = match runtime::run(async move { server.item(key).await }).await {
                    Ok(item) => match item.kind {
                        ItemKind::Artist => "show-artist-by-id",
                        ItemKind::Album => "show-album-by-id",
                        ItemKind::Show => "show-show-by-id",
                        _ => return,
                    },
                    Err(err) => {
                        crate::notify::failure(&crate::messages::open_item(), &err);
                        return;
                    }
                };
                ActionGroupExt::activate_action(&window, action, Some(&key.as_str().to_variant()));
            }
        ));
    }

    fn load_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource(&format!("{}/plex.css", config::RESOURCE_PREFIX));
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("a display is required"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    fn restore_size(&self) {
        let settings = &self.imp().settings;
        self.set_default_size(
            settings.int("window-size-width"),
            settings.int("window-size-height"),
        );
    }

    fn save_size(&self) {
        if self.is_fullscreen() || self.is_maximized() {
            return;
        }
        let settings = &self.imp().settings;
        let (width, height) = self.default_size();
        for (key, value) in [("window-size-width", width), ("window-size-height", height)] {
            if let Err(err) = settings.set_int(key, value) {
                eprintln!("Could not save {key}: {err}");
            }
        }
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        imp._login_view.connect_request(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |request| {
                glib::spawn_future_local(async move { window.login(request).await });
            }
        ));

        imp._login_view.connect_ister_address(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |address| {
                glib::spawn_future_local(async move {
                    let imp = window.imp();
                    let found = imp.session.find_ister(address.clone()).await;
                    // Unless the user typed on in the meantime.
                    if imp._login_view.ister_address() == address {
                        let found = found.as_deref().map_err(|_| ());
                        imp._login_view.set_ister_found(Some(found));
                    }
                });
            }
        ));
        imp._login_view.connect_cancel(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || {
                glib::spawn_future_local(async move { window.cancel_login().await });
            }
        ));
        // The one thing the loading page offers: signing in again where a
        // session ran out.
        imp._loading_view.connect_action(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || {
                let server = window
                    .imp()
                    .session
                    .stored_account()
                    .and_then(|stored| stored.server);
                if let Some(server) = server {
                    let request = LoginRequest::Ister {
                        address: server.url,
                    };
                    glib::spawn_future_local(async move { window.login(request).await });
                }
            }
        ));
        // The sign-in that waits notices that its page was left.
        imp._loading_view.connect_cancel(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || window.show_login()
        ));
        imp._loading_view.connect_accounts(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || window.show_accounts()
        ));

        imp._loading_view.connect_logout(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || {
                glib::spawn_future_local(async move { window.logout().await });
            }
        ));

        imp._sidebar_box.connect_destination(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |destination| {
                // In a narrow window the sidebar lies over the page.
                let split_view = &window.imp()._split_view;
                if split_view.is_collapsed() {
                    split_view.set_show_sidebar(false);
                }
                window.open_root(destination.into());
            }
        ));
        // The sidebar gets out of the way when the window becomes too narrow
        // to show it next to the page, and returns when there is room again.
        imp._split_view.connect_collapsed_notify(|split_view| {
            split_view.set_show_sidebar(!split_view.is_collapsed());
        });

        imp._sidebar_box.connect_server_selected(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |index| {
                let resource = window.imp().servers.borrow().get(index as usize).cloned();
                let Some(resource) = resource else { return };
                let current = window.imp().session.server();
                if current.is_some_and(|s| s.machine_identifier() == resource.client_identifier) {
                    return;
                }
                glib::spawn_future_local(async move { window.use_server(&resource).await });
            }
        ));

        imp._profile_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.show_accounts()
        ));

        imp._search_toggle_button
            .bind_property("active", &*imp._search_bar, "search-mode-enabled")
            .bidirectional()
            .sync_create()
            .build();
        imp._search_bar.connect_entry(&*imp._search_entry);
        // Typing anywhere in the window starts a search.
        imp._search_bar.set_key_capture_widget(Some(self));
        imp._search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |entry| {
                let imp = window.imp();
                if let Some(timeout) = imp.search_timeout.take() {
                    timeout.remove();
                }
                let query = entry.text();
                if query.chars().count() < 3 {
                    return;
                }
                // Wait until typing pauses instead of searching per key.
                let timeout = glib::timeout_add_local_once(
                    SEARCH_DELAY,
                    glib::clone!(
                        #[weak]
                        window,
                        move || {
                            let imp = window.imp();
                            imp.search_timeout.take();
                            window.open(Location::Search(query.to_string()));
                        }
                    ),
                );
                imp.search_timeout.replace(Some(timeout));
            }
        ));
        imp._search_entry.connect_stop_search(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.imp()._search_toggle_button.set_active(false)
        ));

        imp._preferences_button.connect_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.imp()._menu_popover.popdown();
                views::preferences::present(&window);
            }
        ));
        imp._about_button.connect_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.imp()._menu_popover.popdown();
                window.show_about();
            }
        ));
        imp._shortcuts_button.connect_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.imp()._menu_popover.popdown();
                window.show_shortcuts();
            }
        ));

        // Offline sync: the button opens the list, the menu next to it shows
        // running downloads and appears with the first one.
        imp._download_button.set_visible(false);
        imp._download_button
            .set_popover(Some(&DownloadMenu::new(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move || window.imp()._download_button.set_visible(true)
            ))));
        imp._sync_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                if Sync::current().is_busy() {
                    Sync::current().cancel();
                    return;
                }
                window
                    .imp()
                    .sync_dialog
                    .get_or_init(SyncDialog::new)
                    .present(Some(&window));
            }
        ));
        Sync::current().connect(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |event| {
                if let sync::Event::Busy(busy) = event {
                    let imp = window.imp();
                    // While a sync runs the button stops it.
                    imp._sync_image.set_icon_name(Some(if *busy {
                        "process-stop-symbolic"
                    } else {
                        "folder-download-symbolic"
                    }));
                    let tooltip = if *busy {
                        gettext("Stop")
                    } else {
                        gettext("Sync")
                    };
                    imp._sync_button.set_tooltip_text(Some(&tooltip));
                }
            }
        ));
    }

    /// `add-server <url> <token>`, `switch <n>` or `sign-out <n>`, where `n`
    /// counts the accounts as the dialog lists them.
    #[cfg(debug_assertions)]
    pub fn debug_account(&self, step: &str) {
        let mut words = step.split_whitespace().map(str::to_owned);
        let (action, first, second) = (words.next(), words.next(), words.next());
        let nth = |index: &Option<String>| {
            let accounts = self.imp().session.accounts();
            accounts
                .get(index.as_ref()?.parse::<usize>().ok()?)
                .map(|account| account.id.clone())
        };
        let window = self.clone();
        match (action.as_deref(), nth(&first)) {
            (Some("add-server"), _) => {
                let (Some(url), Some(token)) = (first, second) else {
                    return;
                };
                glib::spawn_future_local(async move {
                    window.login(LoginRequest::Server { url, token }).await;
                });
            }
            (Some("add-ister"), _) => {
                let Some(address) = first else {
                    return;
                };
                glib::spawn_future_local(async move {
                    window.login(LoginRequest::Ister { address }).await;
                });
            }
            (Some("switch"), Some(id)) => {
                glib::spawn_future_local(async move { window.switch_account(&id).await });
            }
            (Some("sign-out"), Some(id)) => {
                glib::spawn_future_local(async move { window.sign_out(&id).await });
            }
            _ => eprintln!("GIRENS_DEBUG_ACCOUNT: cannot do {step:?}"),
        }
    }

    #[cfg(debug_assertions)]
    pub fn debug_waiting(&self, button: &str) {
        let imp = self.imp();
        imp._loading_view.debug_press(button);
        let page = if imp.frame_stack.visible_child()
            == Some(imp._login_view.get().upcast::<gtk::Widget>())
        {
            "the sign-in page"
        } else {
            "another page"
        };
        eprintln!(
            "waiting: pressed {button}, now on {page}, in use: {:?}, clipboard: {:?}",
            imp.session.stored_account().map(|stored| stored.kind),
            self.clipboard().content().is_some()
        );
    }

    #[cfg(debug_assertions)]
    pub fn debug_player(&self, command: &str) {
        let Some(player) = self.imp().player.get() else {
            return;
        };
        let mut words = command.split_whitespace();
        let (command, argument) = (words.next(), words.next().and_then(|word| word.parse::<u64>().ok()));
        match (command, argument) {
            (Some("skip"), _) => player.skip_segment(),
            (Some("next"), _) => player.next(),
            (Some("previous"), _) => player.previous(),
            (Some("seek"), Some(seconds)) => player.seek_to(seconds * 1000),
            (Some("subtitle"), Some(nth)) => {
                let stream = player.loaded_item().and_then(|item| {
                    item.streams(girens_model::model::StreamKind::Subtitle)
                        .nth(nth as usize)
                        .map(|stream| stream.id)
                });
                player.set_subtitle_stream(stream);
            }
            (Some("position"), _) => eprintln!(
                "player: {} ms into {:?}",
                player.position(),
                player.loaded_item().map(|item| item.title)
            ),
            _ => eprintln!("GIRENS_DEBUG_PLAYER: cannot do {command:?}"),
        }
    }

    #[cfg(debug_assertions)]
    pub fn debug_open(&self, dialog: &str) {
        match dialog {
            "preferences" => views::preferences::present(self),
            "shortcuts" => self.show_shortcuts(),
            "about" => self.show_about(),
            "accounts" => self.show_accounts(),
            "login" => {
                self.show_login();
                let address = std::env::var("GIRENS_DEBUG_LOGIN_ADDRESS")
                    .unwrap_or_else(|_| "media.example.org".into());
                self.imp()._login_view.offer_ister(&address);
            }
            "pin" | "wrong-pin" => {
                let (window, wrong) = (self.clone(), dialog == "wrong-pin");
                glib::spawn_future_local(async move {
                    let profile = crate::store::Profile {
                        title: "Kids".into(),
                        protected: true,
                        ..Default::default()
                    };
                    window.ask_pin(&profile, wrong).await;
                });
            }
            "sync" => self
                .imp()
                .sync_dialog
                .get_or_init(SyncDialog::new)
                .present(Some(self)),
            _ => {}
        }
    }

    fn show_about(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name("Girens")
            .application_icon(config::APP_ID)
            .version(config::VERSION)
            .developer_name("Gerben Droogers")
            .developers(["Gerben Droogers"])
            .artists(["Tobias Bernard"])
            .copyright("© 2018-2026 Gerben Droogers.")
            .comments(gettext(
                "A Plex GTK client for playing movies, TV shows and music from your Plex library.",
            ))
            .website("https://gitlab.gnome.org/tijder/girens")
            .license_type(gtk::License::Gpl30)
            // Translators put their names here.
            .translator_credits(gettext("translator-credits"))
            .build();
        dialog.add_link(
            &gettext("Help with translating"),
            "https://hosted.weblate.org/projects/girens/girens/",
        );
        dialog.present(Some(self));
    }

    fn show_shortcuts(&self) {
        let builder =
            gtk::Builder::from_resource(&format!("{}/shortcuts.ui", config::RESOURCE_PREFIX));
        let Some(shortcuts) = builder.object::<adw::Dialog>("shortcuts") else {
            eprintln!("The shortcuts dialog is missing from the resources");
            return;
        };
        shortcuts.present(Some(self));
    }

    // Views of the outer stack

    fn show_loading(&self, text: &str) {
        let imp = self.imp();
        imp._loading_view.set_text(text);
        imp._loading_view.set_waiting(None);
        imp._loading_view.set_action(None);
        imp._loading_view
            .set_accounts_visible(imp.session.accounts().len() > 1);
        imp.frame_stack.set_visible_child(&*imp._loading_view);
    }

    /// Shows that Girens waits for the user to sign in at `url` in the
    /// browser, with a way to get to that page again and to give up.
    fn show_waiting_for_browser(&self, url: &str) {
        self.show_loading(&gettext("Waiting for you to sign in in the browser…"));
        let view = &self.imp()._loading_view;
        view.set_waiting(Some(url));
        // Who is signed in already is not what this is about.
        view.set_accounts_visible(false);
    }

    fn show_login(&self) {
        let imp = self.imp();
        imp._login_view.set_busy(false);
        imp._login_view
            .set_cancellable(imp.session.account_id().is_some());
        imp.frame_stack.set_visible_child(&*imp._login_view);
        #[cfg(debug_assertions)]
        crate::debug::account_step(self);
    }

    fn show_main(&self) {
        let imp = self.imp();
        imp.frame_stack.set_visible_child(&*imp._split_view);
    }

    // Actions

    /// The `win.*` actions used by cover tiles, rows and menus. All take the
    /// id of an item as a string.
    fn setup_actions(&self) {
        let add = |name: &str, handler: fn(&Self, ItemId)| {
            let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            action.connect_activate(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_, parameter| {
                    if let Some(key) = parameter.and_then(|value| value.str()) {
                        handler(&window, ItemId::new(key));
                    }
                }
            ));
            self.add_action(&action);
        };

        add("show-show-by-id", |window, key| {
            window.open(Location::Show(key))
        });
        add("show-artist-by-id", |window, key| {
            window.open(Location::Artist(key))
        });
        add("show-album-by-id", |window, key| {
            window.open(Location::Album(key))
        });
        add("show-photo-album-by-id", |window, key| {
            window.open(Location::PhotoAlbum(key))
        });
        add("show-photo-by-id", |window, key| {
            window.open(Location::Photo(key))
        });
        add("show-playlist-by-id", |window, key| {
            window.open(Location::Playlist(key))
        });
        add("mark-as-played-by-id", |window, key| {
            window.set_watched(key, true)
        });
        add("mark-as-unplayed-by-id", |window, key| {
            window.set_watched(key, false)
        });

        add("sync-by-id", |window, key| window.sync_item(key));
        add("play-item", |window, key| {
            window.play(PlayTarget::Item(key), false, StartMode::Ask)
        });
        add("play-item-from-beginning", |window, key| {
            window.play(PlayTarget::Item(key), false, StartMode::Beginning)
        });
        add("shuffle-by-id", |window, key| {
            window.play(PlayTarget::Item(key), true, StartMode::Ask)
        });

        // The ids of a playlist and of the item in it to start at.
        let action = gio::SimpleAction::new("play-playlist-item", glib::VariantTy::new("(ss)").ok());
        action.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, parameter| {
                let keys = parameter
                    .and_then(|value| value.get::<(String, String)>())
                    .map(|(playlist, item)| (ItemId::new(&playlist), ItemId::new(&item)));
                if let Some((playlist, item)) = keys {
                    let target = PlayTarget::PlaylistItem { playlist, item };
                    window.play(target, false, StartMode::Ask);
                }
            }
        ));
        self.add_action(&action);
    }

    /// Adds an item to the offline sync. A show first asks which episodes.
    fn sync_item(&self, rating_key: ItemId) {
        let Some(server) = self.imp().session.server() else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                match runtime::run(async move { server.item(rating_key).await }).await {
                    Ok(item) if item.kind == girens_model::model::ItemKind::Show => {
                        SyncSettings::new(rating_key).present(Some(&window));
                    }
                    Ok(_) => Sync::current().add(SyncEntry {
                        rating_key,
                        converted: false,
                        only_unwatched: false,
                        max_items: None,
                    }),
                    Err(err) => crate::notify::failure(&crate::messages::load_item(), &err),
                }
            }
        ));
    }

    fn set_watched(&self, rating_key: ItemId, watched: bool) {
        let Some(server) = self.imp().session.server() else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                let result = runtime::run(async move {
                    if watched {
                        server.mark_watched(rating_key).await?;
                    } else {
                        server.mark_unwatched(rating_key).await?;
                    }
                    server.item(rating_key).await
                })
                .await;
                match result {
                    Ok(item) => window.item_changed(&item),
                    Err(err) => crate::notify::failure(&crate::messages::change_watched(), &err),
                }
            }
        ));
    }

    /// Shows the new state of an item wherever it is on screen.
    fn item_changed(&self, item: &girens_model::model::Item) {
        use girens_model::model::ItemKind;
        let imp = self.imp();
        imp._section_view.update_item(item);
        imp._search_view.update_item(item);
        imp._player_view.update_item(item);
        if matches!(item.kind, ItemKind::Show | ItemKind::Season) {
            // Their episodes changed too; those rows are loaded again.
            imp._discover_view.refresh();
        } else {
            imp._discover_view.update_item(item);
        }
        let show_visible = imp
            .page_stack
            .visible_child()
            .is_some_and(|page| page == *imp._show_view.upcast_ref::<gtk::Widget>());
        if let (true, Some(key)) = (show_visible, imp._show_view.rating_key()) {
            imp._show_view.show(key);
        }
    }

    // Toasts, layout and shortcuts

    /// Puts the content of the window in a toast overlay for `report!`.
    fn setup_toasts(&self) {
        let overlay = adw::ToastOverlay::new();
        let content = self.content();
        self.set_content(gtk::Widget::NONE);
        overlay.set_child(content.as_ref());
        self.set_content(Some(&overlay));
        crate::notify::set_overlay(&overlay);

        // A failure may mean that the server is gone or the login expired.
        crate::notify::connect_failure(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || {
                glib::spawn_future_local(async move { window.check_connection().await });
            }
        ));
    }

    /// Adapts the pages when the window gets narrower or wider.
    fn setup_layout(&self) {
        self.connect_default_width_notify(|window| {
            let imp = window.imp();
            if let Some(timeout) = imp.layout_timeout.take() {
                timeout.remove();
            }
            let timeout = glib::timeout_add_local_once(
                LAYOUT_DELAY,
                glib::clone!(
                    #[weak]
                    window,
                    move || {
                        window.imp().layout_timeout.take();
                        window.apply_width(window.default_width());
                    }
                ),
            );
            imp.layout_timeout.replace(Some(timeout));
        });
        self.apply_width(self.default_width());
    }

    fn apply_width(&self, width: i32) {
        let imp = self.imp();
        // Two tiles next to each other in a narrow window.
        let cover_width = |margin: i32| if width < 450 { width / 2 - margin } else { 200 };
        imp._discover_view.set_cover_width(cover_width(20));
        imp._section_view.set_cover_width(cover_width(30));
        imp._search_view.set_cover_width(cover_width(30));
        imp._player_view.set_window_width(width);
        if let Some(photos) = imp.photos_view.get() {
            photos.set_cover_width(cover_width(30));
        }
        if let Some(downloads) = imp.downloads_view.get() {
            downloads.set_cover_width(cover_width(30));
        }
        imp._media_box_music.set_narrow(width < 550);
    }

    /// Playback shortcuts work on every page, unless a text field or another
    /// widget uses the key.
    fn setup_shortcuts(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                let typing = gtk::prelude::RootExt::focus(&window)
                    .is_some_and(|widget| widget.is::<gtk::Text>());
                if typing {
                    return glib::Propagation::Proceed;
                }
                // While a photo is shown the arrows step through its album.
                let photo = window.imp().photo_view.get().filter(|view| view.is_mapped());
                if let Some(photo) = photo {
                    if photo.handle_key(key).is_stop() {
                        return glib::Propagation::Stop;
                    }
                }
                window.imp()._player_view.handle_key(key)
            }
        ));
        self.add_controller(keys);
    }
}
