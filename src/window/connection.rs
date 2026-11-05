//! Logging in, connecting to a server and noticing when it goes away.

use super::*;

/// Seconds between checks whether the sign-in in the browser was approved,
/// and how often to check.
const BROWSER_LOGIN_INTERVAL: u32 = 2;
const BROWSER_LOGIN_ATTEMPTS: u32 = 450;

impl PlexWindow {
    /// Restores the previous session, or asks the user to sign in.
    pub(super) async fn start(&self) {
        let session = self.imp().session.clone();
        self.show_loading(&gettext("Starting Girens"));

        // Signing in to plex.tv and connecting to the server used last do not
        // depend on each other, so they run at the same time, and the
        // libraries are shown as soon as the server answers.
        if let Some(url) = session.stored_server_url() {
            self.show_loading(&gettext_f("Connecting to {}.", &[&url]));
        }
        let stored_server = async {
            let stored = session.connect_stored_server().await;
            if matches!(stored, Ok(true)) {
                self.show_library().await;
            }
            stored
        };
        let (account, stored) = tokio::join!(session.restore_account(), stored_server);

        match account {
            Ok(true) => {}
            Ok(false) => {
                if session.stored_server_url().is_none() {
                    self.show_login();
                    return;
                }
            }
            Err(Error::Unauthorized) => {
                self.show_login();
                return;
            }
            // plex.tv is unreachable; the server itself may still be.
            Err(err) => eprintln!("Could not sign in to plex.tv: {err}"),
        }

        match stored {
            Ok(true) => self.load_account_details().await,
            other => {
                let unreachable = matches!(&other, Err(err) if err.is_network());
                if let Err(err) = other {
                    eprintln!("Could not connect to the server used last: {err}");
                }
                if unreachable && self.show_offline() {
                    self.reconnect_from_offline().await;
                } else if self.connect_to_account_server().await {
                    self.connected().await;
                }
            }
        }
    }

    pub(super) async fn login(&self, request: LoginRequest) {
        let imp = self.imp();
        let session = imp.session.clone();
        imp._login_view.set_busy(true);

        let in_browser = matches!(request, LoginRequest::Browser);
        let result = match request {
            LoginRequest::Browser => self.browser_login().await,
            LoginRequest::Account { login, password } => session.login(login, password).await,
            LoginRequest::Server { url, token } => {
                self.show_loading(&gettext_f("Connecting to {}.", &[&url]));
                session.connect_url(url, token).await
            }
        };
        if let Err(err) = result {
            eprintln!("Login failed: {err}");
            self.show_login();
            // Nothing was typed that could be wrong when signing in in the
            // browser was cancelled or timed out.
            imp._login_view.set_error(!in_browser);
            return;
        }

        if session.server().is_some() || self.connect_to_server().await {
            self.connected().await;
        }
    }

    /// Signs in through the browser: opens plex.tv and waits until the user
    /// has approved there. Logging out from the waiting page cancels it.
    async fn browser_login(&self) -> girens_plex::Result<()> {
        let imp = self.imp();
        let session = imp.session.clone();
        let (url, pin) = session.start_browser_login().await?;
        gtk::UriLauncher::new(&url).launch(Some(self), gtk::gio::Cancellable::NONE, |result| {
            if let Err(err) = result {
                eprintln!("Could not open the browser: {err}");
            }
        });
        self.show_loading(&gettext("Waiting for you to sign in in the browser…"));

        // plex.tv keeps a sign-in request for about fifteen minutes.
        for _ in 0..BROWSER_LOGIN_ATTEMPTS {
            glib::timeout_future_seconds(BROWSER_LOGIN_INTERVAL).await;
            let waiting = imp
                .frame_stack
                .visible_child()
                .is_some_and(|child| child == *imp._loading_view.upcast_ref::<gtk::Widget>());
            if !waiting {
                // Cancelled from the waiting page.
                return Err(Error::Unauthorized);
            }
            if session.finish_browser_login(&pin).await? {
                return Ok(());
            }
        }
        Err(Error::Unauthorized)
    }

    /// Connects to the server used last, or else to the first server of the
    /// account that answers. On failure the loading page explains why and
    /// offers to log out.
    pub(super) async fn connect_to_server(&self) -> bool {
        let session = self.imp().session.clone();

        if let Some(url) = session.stored_server_url() {
            self.show_loading(&gettext_f("Connecting to {}.", &[&url]));
            match session.connect_stored_server().await {
                Ok(true) => return true,
                Ok(false) => {}
                Err(err) => {
                    eprintln!("Could not connect to {url}: {err}");
                    self.show_loading(&gettext_f("Connecting to {} failed.", &[&url]));
                }
            }
        }

        self.connect_to_account_server().await
    }

    /// Connects to the first server of the account that answers.
    pub(super) async fn connect_to_account_server(&self) -> bool {
        let session = self.imp().session.clone();
        if session.account().is_none() {
            return false;
        }
        let servers = match session.servers().await {
            Ok(servers) => servers,
            Err(err) => {
                eprintln!("Could not list the servers of the account: {err}");
                return false;
            }
        };
        if servers.is_empty() {
            self.show_loading(&gettext("No servers found for this account."));
            return false;
        }
        for resource in &servers {
            if self.connect_resource(resource).await {
                return true;
            }
        }
        false
    }

    pub(super) async fn connect_resource(&self, resource: &Resource) -> bool {
        let count = resource.connections.len().to_string();
        self.show_loading(&gettext_f(
            "Connecting to {0}.\nThere are {1} connection urls.\nThis may take a while.",
            &[&resource.name, &count],
        ));
        match self.imp().session.connect_resource(resource).await {
            Ok(()) => true,
            Err(err) => {
                eprintln!("Could not connect to {}: {err}", resource.name);
                self.show_loading(&gettext_f("Connecting to {} failed.", &[&resource.name]));
                false
            }
        }
    }

    /// Shows the offline copies when the server cannot be reached. Returns
    /// false when there are none, so that there is nothing to show.
    fn show_offline(&self) -> bool {
        let imp = self.imp();
        let Some(machine) = imp.session.machine_identifier() else {
            return false;
        };
        Sync::current().rescan(&machine);
        if sync::offline_items(&machine).is_empty() {
            return false;
        }
        self.show_main();
        imp._sidebar_box.set_offline();
        crate::notify::message(&gettext(
            "The server cannot be reached. Showing your downloads.",
        ));
        #[cfg(debug_assertions)]
        crate::debug::after_connect(self);
        true
    }

    /// Keeps trying to reach the server while the downloads are shown, and
    /// switches to the normal view when it answers.
    async fn reconnect_from_offline(&self) {
        let session = self.imp().session.clone();
        loop {
            glib::timeout_future_seconds(RECONNECT_INTERVAL).await;
            if session.stored_server_url().is_none() {
                // Logged out in the meantime.
                return;
            }
            if matches!(session.connect_stored_server().await, Ok(true)) {
                self.connected().await;
                return;
            }
        }
    }

    /// Shows the server that was just connected: its libraries first, then
    /// what comes from plex.tv.
    pub(super) async fn connected(&self) {
        self.show_library().await;
        self.load_account_details().await;
    }

    /// Opens the main view with the libraries of the connected server. This
    /// only needs the server itself.
    pub(super) async fn show_library(&self) {
        let imp = self.imp();
        let Some(server) = imp.session.server() else {
            return;
        };
        self.show_main();
        Sync::current().rescan(server.machine_identifier());

        let sections = {
            let server = server.clone();
            runtime::run(async move { server.sections().await }).await
        };
        match sections {
            Ok(sections) => imp._sidebar_box.set_sections(&sections),
            Err(err) => {
                crate::notify::failure(&crate::i18n::gettext("Could not load the libraries"), &err);
                imp._sidebar_box.set_sections(&[]);
            }
        }
        // Until plex.tv has answered, this server is the only one known.
        imp._sidebar_box
            .set_servers(&[server.friendly_name().unwrap_or_default()], Some(0));

        self.open_requested_item();
        crate::search_data::refresh(server);

        #[cfg(debug_assertions)]
        {
            crate::debug::mark("sidebar filled");
            crate::debug::after_connect(self);
        }
    }

    /// Fills in what plex.tv knows: the other servers of the account and the
    /// avatar.
    pub(super) async fn load_account_details(&self) {
        let imp = self.imp();
        let session = imp.session.clone();
        let Some(server) = session.server() else {
            return;
        };
        let servers = session.servers().await.unwrap_or_else(|err| {
            eprintln!("Could not list the servers of the account: {err}");
            Vec::new()
        });
        if !servers.is_empty() {
            let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
            let active = servers
                .iter()
                .position(|s| s.client_identifier == server.machine_identifier());
            imp._sidebar_box
                .set_servers(&names, active.map(|index| index as u32));
        }
        imp.servers.replace(servers);

        // The avatar is decoration; nothing waits for it.
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move { window.load_avatar().await }
        ));
    }

    #[cfg(debug_assertions)]
    pub fn debug_select_sidebar_row(&self, row: i32) {
        self.imp()._sidebar_box.select_row(row);
    }

    pub(super) async fn load_avatar(&self) {
        let imp = self.imp();
        let Some(url) = imp.session.account().and_then(|account| account.thumb) else {
            imp._avatar_image
                .set_icon_name(Some("preferences-desktop-personal-symbolic"));
            return;
        };
        match runtime::run(async move { girens_plex::download(&url).await }).await {
            Ok(bytes) => match gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) {
                Ok(texture) => imp._avatar_image.set_paintable(Some(&texture)),
                Err(err) => eprintln!("Could not decode the avatar: {err}"),
            },
            Err(err) => eprintln!("Could not download the avatar: {err}"),
        }
    }

    pub(super) async fn logout(&self) {
        let imp = self.imp();
        imp.session.logout().await;
        imp.servers.borrow_mut().clear();
        imp._sidebar_box.clear();
        imp._avatar_image
            .set_icon_name(Some("preferences-desktop-personal-symbolic"));
        imp._login_view.reset();
        self.show_login();
    }

    /// Asks the server whether it is still there. If the login is no longer
    /// accepted the login page is shown; if the server cannot be reached the
    /// loading page says so and the connection is retried until it is back.
    pub(super) async fn check_connection(&self) {
        let imp = self.imp();
        let on_main_view = imp
            .frame_stack
            .visible_child()
            .is_some_and(|child| child == *imp._split_view.upcast_ref::<gtk::Widget>());
        if !on_main_view || imp.checking_connection.replace(true) {
            return;
        }
        let mut waiting = false;
        loop {
            let Some(server) = imp.session.server() else {
                break;
            };
            let name = server.base_url().host_str().unwrap_or_default().to_owned();
            match runtime::run(async move { server.sections().await }).await {
                Ok(_) => {
                    if waiting {
                        self.show_main();
                    }
                    break;
                }
                Err(Error::Unauthorized) => {
                    self.show_login();
                    break;
                }
                Err(err) if err.is_network() => {
                    if !waiting {
                        waiting = true;
                        self.show_loading(&gettext_f("Connecting to {} failed.", &[&name]));
                    }
                    glib::timeout_future_seconds(RECONNECT_INTERVAL).await;
                    // The user may have logged out from the loading page.
                    let still_waiting = imp.frame_stack.visible_child().is_some_and(|child| {
                        child == *imp._loading_view.upcast_ref::<gtk::Widget>()
                    });
                    if !still_waiting {
                        break;
                    }
                }
                // The server answers; the failure was about something else.
                Err(_) => {
                    if waiting {
                        self.show_main();
                    }
                    break;
                }
            }
        }
        imp.checking_connection.set(false);
    }
}
