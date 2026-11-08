//! Logging in, switching between accounts and profiles, connecting to a
//! server and noticing when it goes away.

use super::*;
use crate::store::Profile;

/// Seconds between checks whether the sign-in in the browser was approved,
/// and how often to check.
const BROWSER_LOGIN_INTERVAL: u32 = 2;
const BROWSER_LOGIN_ATTEMPTS: u32 = 450;

impl PlexWindow {
    /// Restores the previous session, or asks the user to sign in.
    pub(super) async fn start(&self) {
        let session = self.imp().session.clone();
        self.show_loading(&gettext("Starting Girens"));
        session.prepare().await;
        if session.account_id().is_none() {
            self.show_login();
            return;
        }
        if session.asks_profile_at_start() && self.ask_who_is_watching().await {
            return;
        }

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
        let whose = session.account_id();
        let (account, stored) = tokio::join!(session.restore_account(), stored_server);
        if session.account_id() != whose {
            // Another account was chosen meanwhile, and is starting itself.
            return;
        }

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
            // Unless the connection was dropped again because the profile
            // it was made for could not be continued.
            Ok(true) if session.server().is_some() => self.load_account_details().await,
            // The session at an Ister server ran out: sign in again there.
            Err(Error::Unauthorized) if self.offer_ister_login() => {}
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

    /// Offers to sign in again to the Ister server of the account in use,
    /// when that is what it is.
    fn offer_ister_login(&self) -> bool {
        let stored = self.imp().session.stored_account();
        let Some(server) = stored
            .filter(|stored| stored.kind == crate::store::AccountKind::Ister)
            .and_then(|stored| stored.server)
        else {
            return false;
        };
        // Nothing to fill in: the address is known, only the browser is
        // needed again.
        self.show_loading(&gettext_f(
            "Your session at {} has ended.",
            &[&server.url],
        ));
        self.imp()
            ._loading_view
            .set_action(Some(&gettext("Sign in again")));
        #[cfg(debug_assertions)]
        crate::debug::account_step(self);
        true
    }

    pub(super) async fn login(&self, request: LoginRequest) {
        let imp = self.imp();
        let session = imp.session.clone();
        imp._login_view.set_busy(true);

        let attempt = request.clone();
        let result = match request {
            LoginRequest::Browser => match self.browser_login().await {
                Ok(true) => Ok(()),
                Ok(false) => {
                    self.show_login();
                    imp._login_view.set_error(Some(&gettext(
                        "The sign-in on plex.tv took too long. Try again.",
                    )));
                    return;
                }
                Err(err) => Err(err),
            },
            LoginRequest::Ister { address } => self.ister_login(address).await,
            LoginRequest::Account {
                login,
                password,
                verification_code,
            } => {
                self.show_loading(&gettext("Signing in to plex.tv…"));
                session.login(login, password, verification_code).await
            }
            LoginRequest::Server { url, token } => {
                self.show_loading(&gettext_f("Connecting to {}.", &[&url]));
                session.connect_url(url, token).await
            }
        };
        if let Err(err) = result {
            eprintln!("Login failed: {err}");
            self.show_login();
            imp._login_view
                .set_error(explain_failed_login(&attempt, &err).as_deref());
            return;
        }
        // Whoever was signed in before makes way.
        let server = session.server();
        self.reset_for_user_change();
        imp._login_view.reset();

        if server.is_some() || self.connect_to_server().await {
            self.connected().await;
        }
    }

    /// Signs in through the browser: opens plex.tv and waits until the user
    /// has approved there, or has typed the code of the pin at plex.tv/link
    /// on another device. Logging out from the waiting page cancels it.
    /// `Ok(false)` when plex.tv forgot the pin before the user got to it.
    async fn browser_login(&self) -> girens_model::Result<bool> {
        let imp = self.imp();
        let session = imp.session.clone();
        let signed_in = session.account_id();
        let (url, pin) = session.start_browser_login().await?;
        #[cfg(debug_assertions)]
        let opened = crate::debug::take_login_url(&url);
        #[cfg(not(debug_assertions))]
        let opened = false;
        if !opened {
            gtk::UriLauncher::new(&url).launch(
                Some(self),
                gtk::gio::Cancellable::NONE,
                |result| {
                    if let Err(err) = result {
                        eprintln!("Could not open the browser: {err}");
                    }
                },
            );
        }
        self.show_waiting_for_browser(&url, Some(&pin.code));

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
            if session.account_id() != signed_in {
                // Another account was chosen meanwhile.
                return Err(Error::Unauthorized);
            }
            match session.finish_browser_login(&pin).await {
                Ok(true) => {
                    // The user is still looking at the browser.
                    self.present();
                    return Ok(true);
                }
                Ok(false) => {}
                // A hiccup on the way to plex.tv does not undo the sign-in
                // in progress: the next check may well get through.
                Err(err) if err.is_network() => {
                    eprintln!("Could not check the sign-in on plex.tv: {err}");
                }
                // plex.tv has forgotten the pin.
                Err(Error::Status { status, .. }) if status.as_u16() == 404 => {
                    return Ok(false);
                }
                Err(err) => return Err(err),
            }
        }
        Ok(false)
    }

    /// Signs in to an Ister server: opens the page of its provider in the
    /// browser and waits until the user is done there. Leaving the waiting
    /// page cancels it.
    async fn ister_login(&self, address: String) -> girens_model::Result<()> {
        let imp = self.imp();
        let session = imp.session.clone();
        self.show_loading(&gettext_f("Connecting to {}.", &[&address]));
        let pending = session.start_ister_login(address).await?;
        #[cfg(debug_assertions)]
        let opened = crate::debug::take_login_url(&pending.url);
        #[cfg(not(debug_assertions))]
        let opened = false;
        if !opened {
            gtk::UriLauncher::new(&pending.url).launch(
                Some(self),
                gtk::gio::Cancellable::NONE,
                |result| {
                    if let Err(err) = result {
                        eprintln!("Could not open the browser: {err}");
                    }
                },
            );
        }
        self.show_waiting_for_browser(&pending.url, None);

        let signed_in = session.account_id();
        let cancelled = async {
            loop {
                glib::timeout_future_seconds(1).await;
                let waiting = imp
                    .frame_stack
                    .visible_child()
                    .is_some_and(|child| child == *imp._loading_view.upcast_ref::<gtk::Widget>());
                // Cancelled from the waiting page, or another account was
                // chosen meanwhile.
                if !waiting || session.account_id() != signed_in {
                    return;
                }
            }
        };
        let result = tokio::select! {
            result = session.finish_ister_login(pending) => result,
            () = cancelled => Err(Error::Unauthorized),
        };
        if result.is_ok() {
            // The user is still looking at the browser.
            self.present();
        }
        result
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
                // The session at an Ister server ran out: sign in again.
                Err(Error::Unauthorized) if self.offer_ister_login() => return false,
                Err(err) => {
                    eprintln!("Could not connect to {url}: {err}");
                    self.show_loading(&gettext_f("Connecting to {} failed.", &[&url]));
                }
            }
        }

        self.connect_to_account_server().await
    }

    /// Connects to a server of the account: the one used last if it is
    /// among them, or else the first that answers.
    pub(super) async fn connect_to_account_server(&self) -> bool {
        let session = self.imp().session.clone();
        if session.account().is_none() {
            return false;
        }
        let last = session.machine_identifier();
        let servers = match session.servers().await {
            Ok(mut servers) => {
                servers.sort_by_key(|server| Some(&server.client_identifier) != last.as_ref());
                servers
            }
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

    /// Continues on another server of the account.
    pub(super) async fn use_server(&self, resource: &Resource) {
        if !self.connect_resource(resource).await {
            // Whatever was to be opened there is not opened elsewhere.
            self.imp().launch_options.borrow_mut().show_id = None;
            if self.imp().session.server().is_some() {
                self.show_main();
            }
            return;
        }
        // The other server's pages and playback make way.
        let servers = self.imp().servers.take();
        self.reset_for_user_change();
        self.imp().servers.replace(servers);
        self.connected().await;
    }

    /// Shows the offline copies when the server cannot be reached. Returns
    /// false when there are none, so that there is nothing to show.
    fn show_offline(&self) -> bool {
        let imp = self.imp();
        let (Some(machine), Some(user)) =
            (imp.session.machine_identifier(), imp.session.user_key())
        else {
            return false;
        };
        Sync::current().rescan(&machine, &user);
        if sync::offline_items(&machine, &user).is_empty() {
            return false;
        }
        self.show_main();
        imp._sidebar_box.set_offline();
        self.show_offline_banner();
        #[cfg(debug_assertions)]
        crate::debug::after_connect(self);
        true
    }

    /// Keeps trying to reach the server while the downloads are shown, and
    /// switches to the normal view when it answers.
    async fn reconnect_from_offline(&self) {
        let session = self.imp().session.clone();
        let user = session.user_key();
        loop {
            glib::timeout_future_seconds(RECONNECT_INTERVAL).await;
            if session.stored_server_url().is_none() || session.user_key() != user {
                // Logged out, or someone else took over, in the meantime.
                return;
            }
            if session.server().is_some() {
                // Connected in another way.
                return;
            }
            if self.try_reconnect().await {
                return;
            }
        }
    }

    /// Says above the downloads that the server is away and is tried again.
    fn show_offline_banner(&self) {
        let banner = &self.imp()._offline_banner;
        banner.set_title(&gettext(
            "The server cannot be reached. Showing your downloads until it is back.",
        ));
        banner.set_button_label(Some(&gettext("Try again")));
        banner.set_revealed(true);
    }

    /// One attempt to reach the server again from the downloads, unless
    /// one is running already. Shows the libraries when it answers.
    async fn try_reconnect(&self) -> bool {
        let imp = self.imp();
        if imp.reconnecting.replace(true) {
            return false;
        }
        let connected = matches!(imp.session.connect_stored_server().await, Ok(true));
        imp.reconnecting.set(false);
        if connected {
            self.connected().await;
        }
        connected
    }

    /// The user asks from the banner to try the server now.
    pub(super) async fn reconnect_now(&self) {
        let imp = self.imp();
        let Some(url) = imp.session.stored_server_url() else {
            return;
        };
        if imp.reconnecting.get() {
            return;
        }
        imp._offline_banner
            .set_title(&gettext_f("Connecting to {}…", &[&url]));
        imp._offline_banner.set_button_label(None);
        if !self.try_reconnect().await {
            self.show_offline_banner();
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
        let (Some(server), Some(user)) = (imp.session.server(), imp.session.user_key()) else {
            return;
        };
        self.show_main();
        imp._offline_banner.set_revealed(false);
        Sync::current().rescan(server.machine_identifier(), &user);
        // What was watched from offline copies while the server was away.
        glib::spawn_future_local({
            let (server, user) = (server.clone(), user.clone());
            async move { sync::push_progress(&server, &user).await }
        });

        let downloads = server.supports(crate::backend::Feature::Downloads);
        imp._sync_button.set_visible(downloads);
        let sections = {
            let server = server.clone();
            runtime::run(async move { server.sections().await }).await
        };
        match sections {
            Ok(sections) => imp._sidebar_box.set_sections(&sections, downloads),
            Err(err) => {
                crate::notify::failure(&crate::i18n::gettext("Could not load the libraries"), &err);
                imp._sidebar_box.set_sections(&[], downloads);
            }
        }
        // Until plex.tv has answered, this server is the only one known.
        imp._sidebar_box
            .set_servers(&[server.friendly_name().unwrap_or_default()], Some(0));

        self.open_requested_item();
        crate::search_data::refresh(server, user);

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
        // It may have been opened before plex.tv had answered.
        self.refresh_accounts_dialog();
        // An item of another server of the account may be waiting.
        self.open_requested_item();

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
                .set_icon_name(Some("avatar-default-symbolic"));
            return;
        };
        let whose = imp.session.account_id();
        let downloaded = runtime::run(async move { girens_model::net::download(&url).await }).await;
        if imp.session.account_id() != whose {
            // Someone else took over while the picture was on its way.
            return;
        }
        match downloaded {
            Ok(bytes) => match gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) {
                Ok(texture) => imp._avatar_image.set_paintable(Some(&texture)),
                Err(err) => eprintln!("Could not decode the avatar: {err}"),
            },
            Err(err) => eprintln!("Could not download the avatar: {err}"),
        }
    }

    /// Signs in to the account and asks which profile of its Plex Home
    /// continues. Returns false when plex.tv cannot be asked; the profile
    /// used last then goes on, which is all that works without it.
    async fn ask_who_is_watching(&self) -> bool {
        let session = self.imp().session.clone();
        match session.restore_account_to_choose().await {
            Ok(true) => {}
            Ok(false) => return false,
            Err(Error::Unauthorized) => {
                self.show_login();
                return true;
            }
            Err(err) => {
                eprintln!("Could not sign in to plex.tv: {err}");
                return false;
            }
        }
        self.show_loading(&gettext("Who is watching?"));
        // To open the choice again after closing it.
        self.imp()._loading_view.set_accounts_visible(true);
        self.show_accounts();
        true
    }

    /// Clears away what belongs to whoever was active: playback, a running
    /// sync, the pages that were open and the list of libraries. Used when
    /// another server, profile or account takes over.
    pub(super) fn reset_for_user_change(&self) {
        let imp = self.imp();
        if let Some(player) = imp.player.get() {
            // The play queue and a transcode belong to the session that
            // started them.
            player.stop();
        }
        Sync::current().cancel();
        Sync::current().forget();
        imp.history.borrow_mut().clear();
        imp._section_view.forget();
        if let Some(photos) = imp.photos_view.get() {
            photos.forget();
        }
        if let Some(photo) = imp.photo_view.get() {
            photo.forget();
        }
        imp.servers.borrow_mut().clear();
        imp._sidebar_box.clear();
        imp._offline_banner.set_revealed(false);
        imp._avatar_image
            .set_icon_name(Some("avatar-default-symbolic"));
    }

    /// Shows who is using Girens, to switch profile or account.
    pub(super) fn show_accounts(&self) {
        let dialog = ProfileDialog::new();
        self.imp().accounts_dialog.set(Some(&dialog));

        dialog.connect_profile(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |profile| {
                glib::spawn_future_local(async move { window.switch_profile(profile).await });
            }
        ));
        dialog.connect_account(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |id| {
                glib::spawn_future_local(async move { window.switch_account(&id).await });
            }
        ));
        dialog.connect_remove(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |id| {
                glib::spawn_future_local(async move { window.ask_to_sign_out(&id).await });
            }
        ));
        dialog.connect_add(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || window.add_account()
        ));
        dialog.present(Some(self));
        self.refresh_accounts_dialog();
    }

    /// Fills the dialog with who is using Girens, if it is open: at once
    /// with what is known, and again when plex.tv has listed the profiles.
    pub(super) fn refresh_accounts_dialog(&self) {
        let Some(dialog) = self.imp().accounts_dialog.upgrade() else {
            return;
        };
        let session = self.imp().session.clone();
        // Nobody is, while choosing who is watching at start.
        let active = |session: &Session| session.user_key().filter(|_| session.has_profile());
        dialog.set_account(session.account().as_ref());
        dialog.set_accounts(&session.accounts(), session.account_id().as_deref());
        let known = session
            .stored_account()
            .map(|stored| stored.profiles)
            .unwrap_or_default();
        dialog.set_profiles(&known, active(&session).as_deref());

        glib::spawn_future_local(glib::clone!(
            #[weak]
            dialog,
            async move {
                let account = session.account_id();
                let profiles = session.profiles().await;
                if session.account_id() == account && profiles != known {
                    dialog.set_profiles(&profiles, active(&session).as_deref());
                }
            }
        ));
    }

    /// Asks for the PIN of a protected profile. `None` when the user gives
    /// up.
    pub(super) async fn ask_pin(&self, profile: &Profile, wrong: bool) -> Option<String> {
        let body = if wrong {
            gettext("That PIN is not correct. Try again.")
        } else {
            gettext("Enter the PIN of this profile.")
        };
        let dialog = adw::AlertDialog::new(Some(&profile.title), Some(&body));
        let entry = gtk::Entry::builder()
            .visibility(false)
            .input_purpose(gtk::InputPurpose::Pin)
            .max_length(4)
            .max_width_chars(4)
            .xalign(0.5)
            .halign(gtk::Align::Center)
            .activates_default(true)
            .build();
        entry.update_property(&[gtk::accessible::Property::Label(&gettext("PIN"))]);
        if wrong {
            entry.add_css_class("error");
        }
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("switch", &gettext("Continue"));
        dialog.set_response_appearance("switch", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("switch"));
        dialog.set_close_response("cancel");
        dialog.set_focus(Some(&entry));
        let response = dialog.choose_future(Some(self)).await;
        (response == "switch").then(|| entry.text().to_string())
    }

    /// Continues as another profile of the Plex Home.
    pub(super) async fn switch_profile(&self, profile: Profile) {
        let session = self.imp().session.clone();
        let mut ask = profile.protected;
        let mut wrong = false;
        loop {
            let pin = if ask {
                match self.ask_pin(&profile, wrong).await {
                    Some(pin) => Some(pin),
                    None => return,
                }
            } else {
                None
            };
            match session.switch_profile(&profile, pin).await {
                Ok(()) => break,
                // Also when the profile got a PIN since it was listed.
                Err(Error::WrongPin) => {
                    wrong = ask;
                    ask = true;
                }
                Err(err) => {
                    crate::notify::failure(&gettext("Could not switch to this profile"), &err);
                    return;
                }
            }
        }
        self.reset_for_user_change();
        if self.connect_to_server().await {
            self.connected().await;
        }
    }

    /// Continues with another account that is signed in.
    pub(super) async fn switch_account(&self, id: &str) {
        self.imp().session.switch_account(id);
        self.reset_for_user_change();
        self.start().await;
    }

    /// Offers to sign in with one more account; whoever is signed in stays
    /// until that has worked.
    fn add_account(&self) {
        let imp = self.imp();
        imp._login_view.reset();
        self.show_login();
    }

    /// Returns from the sign-in page to who was signed in.
    pub(super) async fn cancel_login(&self) {
        if self.imp().session.server().is_some() {
            self.show_main();
        } else {
            self.start().await;
        }
    }

    /// Signs an account out after asking: what was kept for it goes too.
    pub(super) async fn ask_to_sign_out(&self, id: &str) {
        let Some(account) = self
            .imp()
            .session
            .accounts()
            .into_iter()
            .find(|account| account.id == id)
        else {
            return;
        };
        let dialog = adw::AlertDialog::new(
            Some(&gettext_f("Sign out {}?", &[&account.name])),
            Some(&gettext(
                "The downloads and preferences of this account are removed from this device.",
            )),
        );
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("logout", &gettext("Sign out"));
        dialog.set_response_appearance("logout", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");
        if dialog.choose_future(Some(self)).await == "logout" {
            self.sign_out(id).await;
        }
    }

    /// Signs an account out. When it was in use, another one that is signed
    /// in takes over, or else the sign-in page is shown.
    pub(super) async fn sign_out(&self, id: &str) {
        let imp = self.imp();
        let session = imp.session.clone();
        let was_active = session.account_id().as_deref() == Some(id);
        session.remove_account(id).await;
        if !was_active {
            return;
        }
        self.reset_for_user_change();
        crate::search_data::clear();
        if session.account_id().is_some() {
            self.start().await;
        } else {
            imp._login_view.reset();
            self.show_login();
        }
    }

    /// Signs out the account in use.
    pub(super) async fn logout(&self) {
        match self.imp().session.account_id() {
            Some(id) => self.sign_out(&id).await,
            None => self.show_login(),
        }
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
            let name = server.host();
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

/// What to tell the user when signing in did not work, which depends on
/// what was tried. `None` when there is nothing to tell: a sign-in in the
/// browser that was cancelled or timed out, where nothing was typed that
/// could be wrong.
fn explain_failed_login(request: &LoginRequest, err: &Error) -> Option<String> {
    match (request, err) {
        (LoginRequest::Browser | LoginRequest::Ister { .. }, Error::Unauthorized) => None,
        (
            LoginRequest::Account {
                verification_code, ..
            },
            Error::Unauthorized,
        ) if verification_code.is_empty() => Some(gettext(
            "The username or password is not right. An account with two-factor authentication also needs its verification code.",
        )),
        (LoginRequest::Account { .. }, Error::Unauthorized) => Some(gettext(
            "The username, password or verification code is not right.",
        )),
        (LoginRequest::Browser | LoginRequest::Account { .. }, err) => {
            Some(gettext_f("Could not reach plex.tv: {}", &[&err.to_string()]))
        }
        (LoginRequest::Server { .. }, Error::Unauthorized) => {
            Some(gettext("The server does not accept this token."))
        }
        (LoginRequest::Server { url, .. }, _) => {
            Some(gettext_f("Could not connect to {}.", &[url]))
        }
        (LoginRequest::Ister { address }, err) if err.is_network() => {
            Some(gettext_f("Could not connect to {}.", &[address]))
        }
        (
            LoginRequest::Ister { address },
            Error::Server(_) | Error::Status { .. } | Error::Decode { .. },
        ) => Some(gettext_f("No Ister server was found at {}.", &[address])),
        (LoginRequest::Ister { .. }, err) => Some(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_sign_in_is_explained_by_what_was_tried() {
        let browser = LoginRequest::Browser;
        let account = LoginRequest::Account {
            login: "me".into(),
            password: "secret".into(),
            verification_code: String::new(),
        };
        let with_code = LoginRequest::Account {
            login: "me".into(),
            password: "secret".into(),
            verification_code: "123456".into(),
        };
        let server = LoginRequest::Server {
            url: "http://nas:32400".into(),
            token: "token".into(),
        };
        let ister = LoginRequest::Ister {
            address: "media.example.org".into(),
        };
        let not_ister = Error::Server("not an Ister server".into());

        // Cancelled in the browser: nothing to say.
        assert_eq!(explain_failed_login(&browser, &Error::Unauthorized), None);
        assert_eq!(explain_failed_login(&ister, &Error::Unauthorized), None);
        assert_eq!(
            explain_failed_login(&account, &Error::Unauthorized).as_deref(),
            Some("The username or password is not right. An account with two-factor authentication also needs its verification code.")
        );
        assert_eq!(
            explain_failed_login(&with_code, &Error::Unauthorized).as_deref(),
            Some("The username, password or verification code is not right.")
        );
        assert_eq!(
            explain_failed_login(&server, &Error::Unauthorized).as_deref(),
            Some("The server does not accept this token.")
        );
        assert_eq!(
            explain_failed_login(&server, &not_ister).as_deref(),
            Some("Could not connect to http://nas:32400.")
        );
        assert_eq!(
            explain_failed_login(&ister, &not_ister).as_deref(),
            Some("No Ister server was found at media.example.org.")
        );
        // The password never ends up in what is shown.
        let shown = explain_failed_login(&account, &not_ister).unwrap();
        assert!(shown.starts_with("Could not reach plex.tv") && !shown.contains("secret"));
    }
}
