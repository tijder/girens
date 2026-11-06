//! Who is signed in: the accounts that are remembered, the one in use, the
//! profile of its Plex Home that is active, and the connected server.
//!
//! All methods are called on the main thread; network work is handed to
//! [`runtime`]. What is remembered between runs lives in [`store`] (who),
//! the settings (which account is in use) and the keyring (the tokens).

use std::cell::RefCell;
use std::rc::Rc;

use girens_plex::myplex::{Account, MyPlex, Resource};
use girens_plex::{ClientInfo, Error, PlexServer, Result};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::store::{AccountKind, Profile, StoredServer};
use crate::{config, runtime, secrets, store, sync};

#[derive(Default)]
struct State {
    /// The account in use; see [`store::Account::id`].
    account_id: Option<String>,
    /// Whose sort orders and offline copies apply.
    user: Option<String>,
    /// The plex.tv session that was signed in with.
    account: Option<MyPlex>,
    /// The plex.tv session of the active profile: `account` itself, or a
    /// member of its Plex Home.
    profile: Option<MyPlex>,
    server: Option<PlexServer>,
}

pub struct Session {
    settings: gio::Settings,
    client: ClientInfo,
    state: RefCell<State>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Session>>> = const { RefCell::new(None) };
}

/// Where the keyring keeps the token of a profile of an account.
fn profile_secret(account_id: &str, profile_uuid: &str) -> String {
    format!("profile:{account_id}:{profile_uuid}")
}

/// Where the keyring keeps the token a user has for a server.
fn server_secret(machine_identifier: &str, user: &str) -> String {
    format!("server:{machine_identifier}:{user}")
}

impl Session {
    /// The session of the application; views use it to reach the server.
    pub fn current() -> Rc<Self> {
        CURRENT.with(|current| {
            current
                .borrow_mut()
                .get_or_insert_with(|| Self::new(&gio::Settings::new(config::APP_ID)))
                .clone()
        })
    }

    fn new(settings: &gio::Settings) -> Rc<Self> {
        let mut identifier = settings.string("client-identifier").to_string();
        if identifier.is_empty() {
            identifier = glib::uuid_string_random().to_string();
            if let Err(err) = settings.set_string("client-identifier", &identifier) {
                eprintln!("Could not save the client identifier: {err}");
            }
        }
        let session = Rc::new(Self {
            settings: settings.clone(),
            client: ClientInfo {
                identifier,
                product: "Girens".into(),
                version: config::VERSION.into(),
                device_name: glib::host_name().to_string(),
            },
            state: RefCell::default(),
        });

        // Earlier versions knew one user; what they stored becomes that
        // user's.
        let server = match (session.setting("server-uuid"), session.setting("server-url")) {
            (Some(uuid), Some(url)) => Some(StoredServer { uuid, url }),
            _ => None,
        };
        let migration = store::migrate(session.setting("user-uuid").as_deref(), server.as_ref());
        if let Some(migration) = migration {
            session.set_setting("user-uuid", &migration.account);
        }
        if let Some(id) = session.setting("user-uuid") {
            session.use_account(&id);
        }
        session
    }

    /// Makes the remembered account `id` the one in use, not signed in yet.
    fn use_account(&self, id: &str) {
        let stored = store::account(id);
        *self.state.borrow_mut() = State {
            user: stored.as_ref().map(|stored| stored.user_key().to_owned()),
            account_id: stored.map(|stored| stored.id),
            ..State::default()
        };
        let active = self.account_id().unwrap_or_default();
        self.set_setting("user-uuid", &active);
    }

    /// The plex.tv details of the active profile.
    pub fn account(&self) -> Option<Account> {
        self.state
            .borrow()
            .profile
            .as_ref()
            .map(|myplex| myplex.account().clone())
    }

    /// Id of the account in use.
    pub fn account_id(&self) -> Option<String> {
        self.state.borrow().account_id.clone()
    }

    /// What is remembered about the account in use.
    pub fn stored_account(&self) -> Option<store::Account> {
        store::account(&self.account_id()?)
    }

    /// The accounts that are signed in.
    pub fn accounts(&self) -> Vec<store::Account> {
        store::accounts()
    }

    /// Key of the active user: sort orders and offline copies are kept per
    /// user.
    pub fn user_key(&self) -> Option<String> {
        self.state.borrow().user.clone()
    }

    pub fn server(&self) -> Option<PlexServer> {
        self.state.borrow().server.clone()
    }

    /// Identifier of the connected server, or of the one used last when
    /// there is no connection; its cache directory is named after it.
    pub fn machine_identifier(&self) -> Option<String> {
        self.server()
            .map(|server| server.machine_identifier().to_owned())
            .or_else(|| Some(self.stored_account()?.server?.uuid))
    }

    fn setting(&self, key: &str) -> Option<String> {
        Some(self.settings.string(key).to_string()).filter(|value| !value.is_empty())
    }

    fn set_setting(&self, key: &str, value: &str) {
        if self.settings.string(key) == value {
            return;
        }
        if let Err(err) = self.settings.set_string(key, value) {
            eprintln!("Could not save {key}: {err}");
        }
    }

    async fn token(&self, key: &str) -> Option<String> {
        match secrets::lookup(key).await {
            Ok(Some(token)) => return Some(token),
            Ok(None) => {}
            Err(err) => eprintln!("Could not read from the keyring: {err}"),
        }
        // The Python version wrote tokens to its config file when no Secret
        // Service was available. Such a token moves to the keyring as soon
        // as that works, and is then removed from the file.
        let token = store::legacy_token(key)?;
        let attributes = vec![("uuid", key.to_owned())];
        match secrets::store("Girens token", attributes, token.clone()).await {
            Ok(()) => store::remove_legacy_token(key),
            Err(err) => eprintln!("Could not move a token into the keyring: {err}"),
        }
        Some(token)
    }

    /// Puts a token of the account `account_id` in the keyring under `key`
    /// and notes that it is there, for when the account signs out.
    async fn keep_token(
        &self,
        account_id: &str,
        label: &'static str,
        mut attributes: Vec<(&'static str, String)>,
        key: String,
        token: String,
    ) {
        // Usually it is there already, from the last time.
        let known = matches!(secrets::lookup(&key).await, Ok(Some(stored)) if stored == token);
        if !known {
            attributes.push(("uuid", key.clone()));
            if let Err(err) = secrets::store(label, attributes, token).await {
                eprintln!("Could not store a token in the keyring: {err}");
                return;
            }
        }
        store::update_account(account_id, |stored| {
            if !stored.secrets.contains(&key) {
                stored.secrets.push(key);
            }
        });
    }

    /// Finishes handing over what earlier versions stored: the token of the
    /// server and the offline copies. Called before anything connects.
    pub async fn prepare(&self) {
        let Some(migration) = store::migrate(None, None) else {
            return;
        };
        if let Some(machine) = &migration.server {
            match secrets::lookup(machine).await {
                Ok(Some(token)) => {
                    let key = server_secret(machine, &migration.user);
                    let attributes = vec![("name", String::new()), ("url", String::new())];
                    self.keep_token(
                        &migration.account,
                        "Girens server token",
                        attributes,
                        key,
                        token,
                    )
                    .await;
                    if let Err(err) = secrets::clear(machine).await {
                        eprintln!("Could not remove a token from the keyring: {err}");
                    }
                }
                Ok(None) => {}
                // Without keyring nothing can be moved now; try again next
                // time.
                Err(err) => {
                    eprintln!("Could not read from the keyring: {err}");
                    return;
                }
            }
            sync::adopt_downloads(machine, &migration.user);
        }
        if migration.account == migration.user {
            store::update_account(&migration.account, |stored| {
                if stored.kind == AccountKind::Plex && !stored.secrets.contains(&stored.id) {
                    stored.secrets.push(stored.id.clone());
                }
            });
        }
        store::finish_migration();
    }

    /// URL of the server that was used last, for status messages.
    pub fn stored_server_url(&self) -> Option<String> {
        Some(self.stored_account()?.server?.url)
    }

    /// Whether to ask who is watching at start, instead of continuing as
    /// the profile that was active: the user wants that, and there is more
    /// than one profile to choose from.
    pub fn asks_profile_at_start(&self) -> bool {
        self.settings.boolean("ask-profile-at-start")
            && self.stored_account().is_some_and(|stored| {
                stored.kind == AccountKind::Plex && stored.profiles.len() > 1
            })
    }

    /// Whether a profile is active. Not between signing in at start and
    /// choosing who is watching.
    pub fn has_profile(&self) -> bool {
        self.state.borrow().profile.is_some()
    }

    /// Signs in to the account in use without continuing as anyone, so that
    /// a profile can be chosen with [`Self::switch_profile`]. `Ok(false)`
    /// means there is no plex.tv account to restore.
    pub async fn restore_account_to_choose(&self) -> Result<bool> {
        let Some(stored) = self
            .stored_account()
            .filter(|stored| stored.kind == AccountKind::Plex)
        else {
            return Ok(false);
        };
        let Some(token) = self.token(&stored.id).await else {
            return Ok(false);
        };
        let client = self.client.clone();
        let account =
            runtime::run(async move { MyPlex::with_token(&token, &client).await }).await?;
        self.remember_details(&account);
        let mut state = self.state.borrow_mut();
        state.account = Some(account);
        state.profile = None;
        state.server = None;
        Ok(true)
    }

    /// Signs in with the tokens of an earlier session: the account in use
    /// and the profile that was active. `Ok(false)` means there is no
    /// plex.tv account to restore.
    pub async fn restore_account(&self) -> Result<bool> {
        let Some(stored) = self
            .stored_account()
            .filter(|stored| stored.kind == AccountKind::Plex)
        else {
            return Ok(false);
        };
        let Some(token) = self.token(&stored.id).await else {
            return Ok(false);
        };
        let client = self.client.clone();
        let account = runtime::run({
            let client = client.clone();
            async move { MyPlex::with_token(&token, &client).await }
        })
        .await?;

        let other_profile = stored.profile.clone().filter(|uuid| *uuid != stored.id);
        let profile = match other_profile {
            None => account.clone(),
            Some(uuid) => {
                let restored = match self.token(&profile_secret(&stored.id, &uuid)).await {
                    Some(token) => {
                        runtime::run(async move { MyPlex::with_token(&token, &client).await }).await
                    }
                    None => Err(Error::Unauthorized),
                };
                match restored {
                    Ok(profile) => profile,
                    Err(err) if err.is_network() => return Err(err),
                    Err(err) => {
                        eprintln!("Could not continue as the profile used last: {err}");
                        // Its own profile is what the account falls back
                        // to, unless that takes a PIN nobody has given.
                        let own_is_protected = stored
                            .profiles
                            .iter()
                            .any(|profile| profile.uuid == stored.id && profile.protected);
                        if own_is_protected {
                            return Err(Error::Unauthorized);
                        }
                        store::update_account(&stored.id, |stored| stored.profile = None);
                        let mut state = self.state.borrow_mut();
                        state.user = Some(stored.id.clone());
                        // A connection made meanwhile was the other profile's.
                        state.server = None;
                        account.clone()
                    }
                }
            }
        };

        self.remember_details(&account);
        let mut state = self.state.borrow_mut();
        state.account = Some(account);
        state.profile = Some(profile);
        Ok(true)
    }

    /// Keeps the name and avatar of an account for the list of accounts.
    fn remember_details(&self, account: &MyPlex) {
        let details = account.account().clone();
        store::update_account(&details.uuid, |stored| {
            stored.name = details
                .title
                .or(details.username)
                .or(details.email)
                .unwrap_or_default();
            stored.thumb = details.thumb;
        });
    }

    pub async fn login(&self, login: String, password: String) -> Result<()> {
        let client = self.client.clone();
        let myplex =
            runtime::run(async move { MyPlex::sign_in(&login, &password, &client).await }).await?;
        self.add_account(myplex).await;
        Ok(())
    }

    /// Starts a sign-in through the browser and returns the page to open
    /// and the pin to wait for.
    pub async fn start_browser_login(&self) -> Result<(String, girens_plex::myplex::Pin)> {
        let client = self.client.clone();
        let pin = runtime::run({
            let client = client.clone();
            async move { MyPlex::create_pin(&client).await }
        })
        .await?;
        Ok((pin.auth_url(&client), pin))
    }

    /// Whether the user has approved `pin` by now; if so the account is
    /// signed in.
    pub async fn finish_browser_login(&self, pin: &girens_plex::myplex::Pin) -> Result<bool> {
        let (client, pin) = (self.client.clone(), pin.clone());
        match runtime::run(async move { MyPlex::check_pin(&pin, &client).await }).await? {
            Some(myplex) => {
                self.add_account(myplex).await;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Remembers an account that just signed in and makes it the one in
    /// use, as itself. Its server is still to be connected.
    async fn add_account(&self, myplex: MyPlex) {
        let details = myplex.account().clone();
        let id = details.uuid.clone();
        let mut stored = store::account(&id).unwrap_or_default();
        stored.id.clone_from(&id);
        stored.kind = AccountKind::Plex;
        stored.profile = None;
        store::save_account(&stored);
        self.remember_details(&myplex);

        self.use_account(&id);
        {
            let mut state = self.state.borrow_mut();
            state.account = Some(myplex.clone());
            state.profile = Some(myplex);
        }
        let attributes = vec![
            ("username", details.username.unwrap_or_default()),
            ("email", details.email.unwrap_or_default()),
        ];
        self.keep_token(&id, "Girens token", attributes, id.clone(), details.auth_token)
            .await;
    }

    /// Connects to the server the account in use used last. `Ok(false)`
    /// means there is none, or the active user has no token for it.
    pub async fn connect_stored_server(&self) -> Result<bool> {
        let (Some(server), Some(user)) = (
            self.stored_account().and_then(|stored| stored.server),
            self.user_key(),
        ) else {
            return Ok(false);
        };
        let Some(token) = self.token(&server_secret(&server.uuid, &user)).await else {
            return Ok(false);
        };
        let client = self.client.clone();
        let connected =
            runtime::run(async move { PlexServer::connect(&server.url, &token, &client).await })
                .await?;
        // Someone else may have become active while this was under way.
        if self.user_key().as_deref() != Some(&user) {
            return Ok(false);
        }
        self.set_server(connected).await;
        Ok(true)
    }

    /// Signs in to a server by address, without a plex.tv account. The
    /// server is remembered as an account of its own.
    pub async fn connect_url(&self, url: String, token: String) -> Result<()> {
        let client = self.client.clone();
        let server =
            runtime::run(async move { PlexServer::connect(&url, &token, &client).await }).await?;
        let id = store::server_account_id(server.machine_identifier());
        let mut stored = store::account(&id).unwrap_or_default();
        stored.id.clone_from(&id);
        stored.kind = AccountKind::Server;
        stored.name = server
            .friendly_name()
            .map(str::to_owned)
            .or_else(|| server.base_url().host_str().map(str::to_owned))
            .unwrap_or_default();
        store::save_account(&stored);
        self.use_account(&id);
        self.set_server(server).await;
        Ok(())
    }

    /// The servers the active profile can use; none without plex.tv account.
    pub async fn servers(&self) -> Result<Vec<Resource>> {
        let Some(myplex) = self.state.borrow().profile.clone() else {
            return Ok(Vec::new());
        };
        runtime::run(async move { myplex.servers().await }).await
    }

    pub async fn connect_resource(&self, resource: &Resource) -> Result<()> {
        let Some(myplex) = self.state.borrow().profile.clone() else {
            return Err(Error::Unauthorized);
        };
        let resource = resource.clone();
        let server = runtime::run(async move { myplex.connect(&resource).await }).await?;
        self.set_server(server).await;
        Ok(())
    }

    async fn set_server(&self, server: PlexServer) {
        let (Some(account_id), Some(user)) = (self.account_id(), self.user_key()) else {
            return;
        };
        let url = server.base_url().as_str().trim_end_matches('/').to_owned();
        let machine = server.machine_identifier().to_owned();
        store::update_account(&account_id, |stored| {
            stored.server = Some(StoredServer {
                uuid: machine.clone(),
                url: url.clone(),
            });
        });
        let attributes = vec![
            (
                "name",
                server.friendly_name().unwrap_or_default().to_owned(),
            ),
            ("url", url),
        ];
        let token = server.token().to_owned();
        self.state.borrow_mut().server = Some(server);
        let key = server_secret(&machine, &user);
        self.keep_token(&account_id, "Girens server token", attributes, key, token)
            .await;
    }

    /// The profiles of the Plex Home of the account in use: from plex.tv,
    /// or as they were listed last when that cannot be reached. Empty for
    /// an account that is a server.
    pub async fn profiles(&self) -> Vec<Profile> {
        let Some(stored) = self.stored_account() else {
            return Vec::new();
        };
        let Some(account) = self.state.borrow().account.clone() else {
            return stored.profiles;
        };
        match runtime::run(async move { account.home_users().await }).await {
            Ok(users) => {
                let profiles: Vec<Profile> = users
                    .into_iter()
                    .map(|user| Profile {
                        uuid: user.uuid,
                        title: user.title,
                        thumb: user.thumb,
                        protected: user.protected,
                        restricted: user.restricted,
                        admin: user.admin,
                    })
                    .collect();
                store::update_account(&stored.id, |stored| {
                    stored.profiles.clone_from(&profiles);
                });
                profiles
            }
            Err(err) => {
                eprintln!("Could not list the profiles of the Plex Home: {err}");
                stored.profiles
            }
        }
    }

    /// Continues as another profile of the Plex Home. `pin` is needed for a
    /// protected one; [`Error::WrongPin`] when it is not right. The server
    /// is still to be connected afterwards, as that profile.
    pub async fn switch_profile(&self, profile: &Profile, pin: Option<String>) -> Result<()> {
        let Some(account) = self.state.borrow().account.clone() else {
            return Err(Error::Unauthorized);
        };
        let id = account.account().uuid.clone();
        let own = profile.uuid == id;
        let session = if own && !profile.protected {
            account
        } else {
            let uuid = profile.uuid.clone();
            runtime::run(async move { account.switch_user(&uuid, pin.as_deref()).await }).await?
        };
        let token = session.account().auth_token.clone();
        store::update_account(&id, |stored| {
            stored.profile = (!own).then(|| profile.uuid.clone());
        });
        {
            let mut state = self.state.borrow_mut();
            state.user = Some(profile.uuid.clone());
            state.profile = Some(session);
            state.server = None;
        }
        if !own {
            let key = profile_secret(&id, &profile.uuid);
            self.keep_token(&id, "Girens profile token", Vec::new(), key, token)
                .await;
        }
        Ok(())
    }

    /// Makes another remembered account the one in use. It is to be
    /// restored and connected afterwards, as at start.
    pub fn switch_account(&self, id: &str) {
        self.use_account(id);
    }

    /// Signs the account `id` out: forgets it, its tokens and what was kept
    /// for its users. When it was in use, another account takes its place
    /// if there is one.
    pub async fn remove_account(&self, id: &str) {
        let Some(stored) = store::account(id) else {
            return;
        };
        let mut keys = stored.secrets.clone();
        // Also when the list of tokens was never written.
        keys.push(stored.id.clone());
        keys.dedup();
        for key in keys {
            if let Err(err) = secrets::clear(&key).await {
                eprintln!("Could not remove a token from the keyring: {err}");
            }
        }
        let gone = store::remove_account(id);
        sync::forget_users(&gone);
        if self.account_id().as_deref() == Some(id) {
            let next = store::accounts().into_iter().next();
            self.use_account(next.map(|next| next.id).as_deref().unwrap_or_default());
        }
    }
}

/// `$XDG_CONFIG_HOME/config`; see [`store`].
pub fn config_file() -> std::path::PathBuf {
    glib::user_config_dir().join("config")
}
