//! Who is signed in: the accounts that are remembered, the one in use, the
//! profile of its Plex Home that is active, and the connected server.
//!
//! All methods are called on the main thread; network work is handed to
//! [`runtime`]. What is remembered between runs lives in [`store`] (who),
//! the settings (which account is in use) and the keyring (the tokens).

use std::cell::RefCell;
use std::rc::Rc;

use girens_plex::myplex::{Account, MyPlex, Resource};
use girens_ister::auth::{Provider, Tokens, WellKnown};
use girens_ister::IsterServer;
use girens_model::{Error, Result};
use girens_plex::{ClientInfo, PlexServer};

use crate::backend::Server;
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
    server: Option<Server>,
}

/// A sign-in to an Ister server that waits for the browser.
pub struct IsterLogin {
    /// The page to sign in at.
    pub url: String,
    address: String,
    well_known: WellKnown,
    provider: Provider,
    login: girens_ister::auth::Login,
}

pub struct Session {
    settings: gio::Settings,
    client: ClientInfo,
    /// Where plex.tv is; a stand-in under test.
    plex_tv: String,
    state: RefCell<State>,
}

const PLEX_TV: &str = "https://plex.tv";

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
                .get_or_insert_with(|| Self::new(&gio::Settings::new(config::APP_ID), PLEX_TV))
                .clone()
        })
    }

    fn new(settings: &gio::Settings, plex_tv: &str) -> Rc<Self> {
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
            plex_tv: plex_tv.to_owned(),
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

    pub fn server(&self) -> Option<Server> {
        self.state.borrow().server.clone()
    }

    /// Whether the account in use can keep copies for watching offline.
    pub fn can_download(&self) -> bool {
        match self.server() {
            Some(server) => server.supports(crate::backend::Feature::Downloads),
            // Without a connection: by what kind of account it is.
            None => self
                .stored_account()
                .is_some_and(|stored| stored.kind != AccountKind::Ister),
        }
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
                    Self::keep_token(
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
        let (client, plex_tv) = (self.client.clone(), self.plex_tv.clone());
        let account =
            runtime::run(async move { MyPlex::with_token_at(&plex_tv, &token, &client).await })
                .await?;
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
        let (client, plex_tv) = (self.client.clone(), self.plex_tv.clone());
        let account = runtime::run({
            let (client, plex_tv) = (client.clone(), plex_tv.clone());
            async move { MyPlex::with_token_at(&plex_tv, &token, &client).await }
        })
        .await?;

        let other_profile = stored.profile.clone().filter(|uuid| *uuid != stored.id);
        let profile = match other_profile {
            None => account.clone(),
            Some(uuid) => {
                let restored = match self.token(&profile_secret(&stored.id, &uuid)).await {
                    Some(token) => {
                        runtime::run(async move {
                            MyPlex::with_token_at(&plex_tv, &token, &client).await
                        })
                        .await
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
        Self::keep_token(&id, "Girens token", attributes, id.clone(), details.auth_token)
            .await;
    }

    // Ister

    fn http() -> reqwest::Client {
        static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        CLIENT.get_or_init(reqwest::Client::new).clone()
    }

    /// Finds the Ister server at `address` and prepares signing in to it in
    /// the browser. When someone is signed in already, the provider is told
    /// to ask who it is this time, so that another user can be added.
    pub async fn start_ister_login(&self, address: String) -> Result<IsterLogin> {
        let ask_again = self.account_id().is_some();
        runtime::run(async move {
            let http = Self::http();
            let well_known = girens_ister::auth::discover(&http, &address).await?;
            let provider = Provider::fetch(&http, &well_known.openid_url).await?;
            let login = girens_ister::auth::Login::start(provider.clone(), ask_again).await?;
            Ok(IsterLogin {
                url: login.url.clone(),
                address,
                well_known,
                provider,
                login,
            })
        })
        .await
    }

    /// Waits until the user has signed in in the browser, then connects and
    /// remembers the account. Dropping the future gives up the sign-in.
    pub async fn finish_ister_login(&self, pending: IsterLogin) -> Result<()> {
        let IsterLogin {
            address,
            well_known,
            provider,
            login,
            ..
        } = pending;
        let tokens = runtime::run(async move { login.finish(&Self::http()).await }).await?;
        let server = self.open_ister(well_known, provider, tokens, None).await?;

        let user = server.user().clone();
        let id = store::ister_account_id(server.machine_identifier(), &user.id);
        let mut stored = store::account(&id).unwrap_or_default();
        stored.id.clone_from(&id);
        stored.kind = AccountKind::Ister;
        stored.name = user.name;
        stored.profile = None;
        stored.server = Some(StoredServer {
            uuid: server.machine_identifier().to_owned(),
            url: address,
        });
        store::save_account(&stored);
        self.use_account(&id);
        self.set_ister_server(&id, server).await;
        Ok(())
    }

    /// Connects to an Ister server. `account` is whose refresh token to
    /// replace in the keyring when the provider hands out a new one; a
    /// sign-in does not know that yet and keeps the token itself.
    async fn open_ister(
        &self,
        well_known: WellKnown,
        provider: Provider,
        tokens: Tokens,
        account: Option<String>,
    ) -> Result<IsterServer> {
        let keeper: girens_ister::TokenKeeper = std::sync::Arc::new(move |token: &str| {
            let (Some(account), token) = (account.clone(), token.to_owned()) else {
                return;
            };
            // Called from the network threads; the keyring is used from the
            // main loop.
            glib::MainContext::default().invoke(move || {
                glib::spawn_future_local(async move {
                    Self::keep_ister_token(&account, token).await;
                });
            });
        });
        // Titles in the languages of the desktop, most wanted first.
        let languages = glib::language_names()
            .iter()
            .filter_map(|name| name.split(['_', '.', '@']).next())
            .filter(|language| language.len() == 2 || language.len() == 3)
            .map(str::to_owned)
            .fold(Vec::new(), |mut seen: Vec<String>, language| {
                if !seen.contains(&language) {
                    seen.push(language);
                }
                seen
            });
        let device = self.client.identifier.clone();
        runtime::run(async move {
            IsterServer::connect(
                Self::http(),
                well_known,
                provider,
                tokens,
                languages,
                device,
                keeper,
            )
            .await
        })
        .await
    }

    async fn keep_ister_token(account_id: &str, token: String) {
        let attributes = vec![("kind", "ister".to_owned())];
        Self::keep_token(
            account_id,
            "Girens Ister token",
            attributes,
            account_id.to_owned(),
            token,
        )
        .await;
    }

    async fn set_ister_server(&self, account_id: &str, server: IsterServer) {
        let token = runtime::run({
            let server = server.clone();
            async move { server.refresh_token().await }
        })
        .await;
        self.state.borrow_mut().server = Some(server.into());
        Self::keep_ister_token(account_id, token).await;
    }

    /// Continues the session of an Ister account with the token kept from
    /// last time. [`Error::Unauthorized`] means it ran out: sign in again.
    async fn connect_stored_ister(&self, stored: store::Account) -> Result<bool> {
        let Some(server) = stored.server.clone() else {
            return Ok(false);
        };
        let Some(refresh) = self.token(&stored.id).await else {
            return Err(Error::Unauthorized);
        };
        let (well_known, provider, tokens) = runtime::run(async move {
            let http = Self::http();
            let well_known = girens_ister::auth::discover(&http, &server.url).await?;
            let provider = Provider::fetch(&http, &well_known.openid_url).await?;
            let tokens = girens_ister::auth::refresh(&http, &provider, &refresh).await?;
            Ok::<_, Error>((well_known, provider, tokens))
        })
        .await?;
        let connected = self
            .open_ister(well_known, provider, tokens, Some(stored.id.clone()))
            .await?;
        // Someone else may have become active while this was under way.
        if self.account_id().as_deref() != Some(&stored.id) {
            return Ok(false);
        }
        self.set_ister_server(&stored.id, connected).await;
        Ok(true)
    }

    /// Signs the Ister session of `stored` out at its provider, as far as
    /// that can be reached.
    async fn end_ister_session(&self, stored: &store::Account) {
        let (Some(server), Some(refresh)) = (stored.server.clone(), self.token(&stored.id).await)
        else {
            return;
        };
        let ended = runtime::run(async move {
            let http = Self::http();
            let well_known = girens_ister::auth::discover(&http, &server.url).await?;
            let provider = Provider::fetch(&http, &well_known.openid_url).await?;
            girens_ister::auth::revoke(&http, &provider, &refresh).await
        })
        .await;
        if let Err(err) = ended {
            eprintln!("Could not sign out at the provider: {err}");
        }
    }

    /// Connects to the server the account in use used last. `Ok(false)`
    /// means there is none, or the active user has no token for it.
    pub async fn connect_stored_server(&self) -> Result<bool> {
        if let Some(stored) = self
            .stored_account()
            .filter(|stored| stored.kind == AccountKind::Ister)
        {
            return self.connect_stored_ister(stored).await;
        }
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
        self.state.borrow_mut().server = Some(server.into());
        let key = server_secret(&machine, &user);
        Self::keep_token(&account_id, "Girens server token", attributes, key, token)
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
            Self::keep_token(&id, "Girens profile token", Vec::new(), key, token)
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
        if stored.kind == AccountKind::Ister {
            self.end_ister_session(&stored).await;
        }
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

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::path::PathBuf;

    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn fixture(name: &str) -> String {
        let file = format!(
            "{}/crates/girens-plex/tests/fixtures/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&file).unwrap_or_else(|err| panic!("{file}: {err}"))
    }

    fn json(status: u16, body: String) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_raw(body, "application/json")
    }

    /// The settings of Girens, kept in memory.
    fn settings() -> gio::Settings {
        static SCHEMAS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let dir = SCHEMAS.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("girens-schemas-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let compiled = std::process::Command::new("glib-compile-schemas")
                .arg(format!("--targetdir={}", dir.display()))
                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/data"))
                .status()
                .expect("glib-compile-schemas is needed for this test");
            assert!(compiled.success());
            dir
        });
        let source = gio::SettingsSchemaSource::from_directory(dir, None, false).unwrap();
        let schema = source.lookup(config::APP_ID, false).unwrap();
        let backend = gio::memory_settings_backend_new();
        gio::Settings::new_full(&schema, Some(&backend), None)
    }

    /// Runs `test` with a config file and a keyring of its own.
    fn run<F: Future<Output = ()>>(name: &str, test: impl FnOnce() -> F) {
        let file = std::env::temp_dir().join(format!(
            "girens-session-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&file);
        store::use_test_file(Some(file.clone()));
        secrets::TEST_KEYRING.with_borrow_mut(|keyring| keyring.clear());
        glib::MainContext::new().block_on(test());
        store::use_test_file(None);
        let _ = std::fs::remove_file(file);
    }

    fn keyring() -> Vec<String> {
        let mut keys: Vec<String> =
            secrets::TEST_KEYRING.with_borrow(|keyring| keyring.keys().cloned().collect());
        keys.sort();
        keys
    }

    fn secret(key: &str) -> Option<String> {
        secrets::TEST_KEYRING.with_borrow(|keyring| keyring.get(key).cloned())
    }

    /// A Plex Media Server that accepts `token`. It is never dropped:
    /// wiremock cannot shut a server down from within the main loop the
    /// tests run in.
    async fn media_server(machine: &str, token: &'static str) -> &'static MockServer {
        let root = fixture("server_root").replace("machineIdentifier-04409eeb", machine);
        runtime::run(async move {
            let mock = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/"))
                .and(header("x-plex-token", token))
                .respond_with(json(200, root))
                .mount(&mock)
                .await;
            Mock::given(method("GET"))
                .and(path("/"))
                .respond_with(ResponseTemplate::new(401))
                .with_priority(10)
                .mount(&mock)
                .await;
            &*Box::leak(Box::new(mock))
        })
        .await
    }

    /// A plex.tv with the account `uuid-1`, whose Home also has `uuid-2`
    /// (with PIN 1234) and the managed `uuid-3`.
    async fn plex_tv() -> &'static MockServer {
        let user = fixture("plextv_user");
        let home = fixture("plextv_home_users");
        let switched = fixture("plextv_switch");
        runtime::run(async move {
            let mock = MockServer::start().await;
            for (token, body) in [("account-token", &user), ("profile-token", &switched)] {
                Mock::given(method("GET"))
                    .and(path("/api/v2/user"))
                    .and(header("x-plex-token", token))
                    .respond_with(json(200, body.clone()))
                    .mount(&mock)
                    .await;
            }
            Mock::given(method("GET"))
                .and(path("/api/v2/user"))
                .respond_with(ResponseTemplate::new(401))
                .with_priority(10)
                .mount(&mock)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v2/home/users"))
                .respond_with(json(200, home))
                .mount(&mock)
                .await;
            Mock::given(method("POST"))
                .and(path("/api/v2/home/users/uuid-3/switch"))
                .respond_with(json(201, switched.clone()))
                .mount(&mock)
                .await;
            Mock::given(method("POST"))
                .and(path("/api/v2/home/users/uuid-2/switch"))
                .and(query_param("pin", "1234"))
                .respond_with(json(201, switched.replace("uuid-3", "uuid-2")))
                .mount(&mock)
                .await;
            Mock::given(method("POST"))
                .and(path("/api/v2/home/users/uuid-2/switch"))
                .respond_with(ResponseTemplate::new(403))
                .with_priority(10)
                .mount(&mock)
                .await;
            &*Box::leak(Box::new(mock))
        })
        .await
    }

    /// What an earlier run left behind for a signed-in plex.tv account.
    async fn signed_in(settings: &gio::Settings) {
        store::save_account(&store::Account {
            id: "uuid-1".into(),
            ..store::Account::default()
        });
        settings.set_string("user-uuid", "uuid-1").unwrap();
        let attributes = vec![("uuid", "uuid-1".to_owned())];
        secrets::store("Girens token", attributes, "account-token".into())
            .await
            .unwrap();
    }

    #[test]
    fn servers_by_address_are_accounts_to_switch_between_and_sign_out() {
        run("servers", || async {
            let first = media_server("first", "token-1").await;
            let second = media_server("second", "token-2").await;
            let session = Session::new(&settings(), PLEX_TV);
            assert!(session.account_id().is_none());

            assert!(matches!(
                session.connect_url(first.uri(), "wrong".into()).await,
                Err(Error::Unauthorized)
            ));
            assert!(session.accounts().is_empty());

            session
                .connect_url(first.uri(), "token-1".into())
                .await
                .unwrap();
            assert_eq!(session.account_id().as_deref(), Some("server:first"));
            assert_eq!(session.user_key().as_deref(), Some("server:first"));
            assert_eq!(session.machine_identifier().as_deref(), Some("first"));

            // A second one does not replace the first.
            session
                .connect_url(second.uri(), "token-2".into())
                .await
                .unwrap();
            assert_eq!(session.account_id().as_deref(), Some("server:second"));
            assert_eq!(session.accounts().len(), 2);
            assert_eq!(
                keyring(),
                ["server:first:server:first", "server:second:server:second"]
            );

            // Back to the first, as at start: nothing connected yet.
            session.switch_account("server:first");
            assert!(session.server().is_none());
            assert_eq!(session.user_key().as_deref(), Some("server:first"));
            assert!(!session.restore_account().await.unwrap());
            assert!(session.connect_stored_server().await.unwrap());
            assert_eq!(session.machine_identifier().as_deref(), Some("first"));

            // Signing out the one in use leaves the other, which takes over.
            store::add_sync_entry(
                "server:first",
                "first",
                &store::SyncEntry {
                    rating_key: 1.into(),
                    converted: false,
                    only_unwatched: false,
                    max_items: None,
                },
            );
            session.remove_account("server:first").await;
            assert_eq!(session.account_id().as_deref(), Some("server:second"));
            assert!(session.server().is_none());
            assert_eq!(keyring(), ["server:second:server:second"]);
            assert!(store::sync_entries("server:first", "first").is_empty());
            assert!(session.connect_stored_server().await.unwrap());
            assert_eq!(session.machine_identifier().as_deref(), Some("second"));

            // Signing out the last one leaves nobody.
            session.remove_account("server:second").await;
            assert!(session.account_id().is_none());
            assert!(session.user_key().is_none());
            assert!(session.server().is_none());
            assert!(session.accounts().is_empty());
            assert!(keyring().is_empty());
            assert!(!session.connect_stored_server().await.unwrap());
        });
    }

    #[test]
    fn a_profile_of_the_home_takes_over_and_is_continued_next_time() {
        run("profiles", || async {
            let plex_tv = plex_tv().await;
            let settings = settings();
            signed_in(&settings).await;

            let session = Session::new(&settings, &plex_tv.uri());
            assert_eq!(session.user_key().as_deref(), Some("uuid-1"));
            assert!(!session.has_profile());
            assert!(session.restore_account().await.unwrap());
            assert_eq!(session.account().unwrap().uuid, "uuid-1");
            assert_eq!(session.stored_account().unwrap().name, "title-1");

            let profiles = session.profiles().await;
            assert_eq!(profiles.len(), 3);
            assert_eq!(session.stored_account().unwrap().profiles, profiles);

            // A wrong PIN changes nothing.
            let wrong = session.switch_profile(&profiles[1], Some("0000".into())).await;
            assert!(matches!(wrong, Err(Error::WrongPin)));
            assert_eq!(session.user_key().as_deref(), Some("uuid-1"));
            assert_eq!(session.account().unwrap().uuid, "uuid-1");

            session.switch_profile(&profiles[2], None).await.unwrap();
            assert_eq!(session.user_key().as_deref(), Some("uuid-3"));
            assert_eq!(session.account().unwrap().uuid, "uuid-3");
            assert_eq!(session.account_id().as_deref(), Some("uuid-1"));
            assert_eq!(
                secret("profile:uuid-1:uuid-3").as_deref(),
                Some("profile-token")
            );

            // The next run continues as that profile.
            let next = Session::new(&settings, &plex_tv.uri());
            assert_eq!(next.user_key().as_deref(), Some("uuid-3"));
            assert!(next.restore_account().await.unwrap());
            assert_eq!(next.account().unwrap().uuid, "uuid-3");

            // And back to the account itself, which needs no PIN here.
            next.switch_profile(&profiles[0], None).await.unwrap();
            assert_eq!(next.user_key().as_deref(), Some("uuid-1"));
            assert_eq!(next.stored_account().unwrap().profile, None);

            // With the right PIN the protected profile opens.
            next.switch_profile(&profiles[1], Some("1234".into()))
                .await
                .unwrap();
            assert_eq!(next.user_key().as_deref(), Some("uuid-2"));

            // Signing out removes every token of the account.
            next.remove_account("uuid-1").await;
            assert!(keyring().is_empty());
            assert!(next.account().is_none());
            assert!(next.account_id().is_none());
        });
    }

    #[test]
    fn a_profile_that_cannot_be_continued_falls_back_to_the_account() {
        run("fallback", || async {
            let plex_tv = plex_tv().await;
            let settings = settings();
            signed_in(&settings).await;
            let session = Session::new(&settings, &plex_tv.uri());
            session.restore_account().await.unwrap();
            let profiles = session.profiles().await;
            session.switch_profile(&profiles[2], None).await.unwrap();

            // Its token is gone, e.g. removed from the keyring.
            secrets::clear("profile:uuid-1:uuid-3").await.unwrap();
            let next = Session::new(&settings, &plex_tv.uri());
            assert!(next.restore_account().await.unwrap());
            assert_eq!(next.user_key().as_deref(), Some("uuid-1"));
            assert_eq!(next.account().unwrap().uuid, "uuid-1");
            assert_eq!(next.stored_account().unwrap().profile, None);

            // Not when the account itself is behind a PIN: then nobody is
            // let in without signing in again.
            next.switch_profile(&profiles[2], None).await.unwrap();
            secrets::clear("profile:uuid-1:uuid-3").await.unwrap();
            store::update_account("uuid-1", |stored| stored.profiles[0].protected = true);
            let locked = Session::new(&settings, &plex_tv.uri());
            assert!(matches!(
                locked.restore_account().await,
                Err(Error::Unauthorized)
            ));
            assert!(locked.account().is_none());
        });
    }

    #[test]
    fn asking_who_is_watching_signs_in_without_a_profile() {
        run("choose", || async {
            let plex_tv = plex_tv().await;
            let settings = settings();
            signed_in(&settings).await;
            let session = Session::new(&settings, &plex_tv.uri());
            // Not asked for a single profile, or when the user does not
            // want it.
            settings.set_boolean("ask-profile-at-start", true).unwrap();
            assert!(!session.asks_profile_at_start());
            session.restore_account().await.unwrap();
            let profiles = session.profiles().await;
            assert!(session.asks_profile_at_start());
            settings.set_boolean("ask-profile-at-start", false).unwrap();
            assert!(!session.asks_profile_at_start());

            let next = Session::new(&settings, &plex_tv.uri());
            assert!(next.restore_account_to_choose().await.unwrap());
            assert!(!next.has_profile());
            assert!(next.account().is_none());
            next.switch_profile(&profiles[2], None).await.unwrap();
            assert!(next.has_profile());
            assert_eq!(next.user_key().as_deref(), Some("uuid-3"));
        });
    }

    #[test]
    fn what_a_single_user_version_stored_is_handed_over() {
        run("migration", || async {
            let server = media_server("machine", "server-token").await;
            let settings = settings();
            settings.set_string("user-uuid", "uuid-1").unwrap();
            settings.set_string("server-uuid", "machine").unwrap();
            settings.set_string("server-url", &server.uri()).unwrap();
            for (key, token) in [("uuid-1", "account-token"), ("machine", "server-token")] {
                let attributes = vec![("uuid", key.to_owned())];
                secrets::store("Girens token", attributes, token.into())
                    .await
                    .unwrap();
            }

            let session = Session::new(&settings, PLEX_TV);
            assert_eq!(session.account_id().as_deref(), Some("uuid-1"));
            // Until the keyring is done the server cannot be connected.
            assert!(!session.connect_stored_server().await.unwrap());
            session.prepare().await;
            assert_eq!(keyring(), ["server:machine:uuid-1", "uuid-1"]);
            assert!(session.connect_stored_server().await.unwrap());
            assert_eq!(session.machine_identifier().as_deref(), Some("machine"));

            // Once.
            assert!(store::migrate(None, None).is_none());
            let mut secrets = session.stored_account().unwrap().secrets;
            secrets.sort();
            assert_eq!(secrets, ["server:machine:uuid-1", "uuid-1"]);
        });
    }
}
