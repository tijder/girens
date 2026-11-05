//! The login state: the plex.tv account (if any) and the connected server.
//!
//! Replaces the login and connection half of `legacy/plex.py`. All methods
//! are called on the main thread; network work is handed to [`runtime`].

use std::cell::RefCell;
use std::rc::Rc;

use girens_plex::myplex::{Account, MyPlex, Resource};
use girens_plex::{ClientInfo, Error, PlexServer, Result};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::{config, runtime, secrets, store};

#[derive(Default)]
struct State {
    account: Option<MyPlex>,
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
        Rc::new(Self {
            settings: settings.clone(),
            client: ClientInfo {
                identifier,
                product: "Girens".into(),
                version: config::VERSION.into(),
                device_name: glib::host_name().to_string(),
            },
            state: RefCell::default(),
        })
    }

    pub fn account(&self) -> Option<Account> {
        self.state
            .borrow()
            .account
            .as_ref()
            .map(|myplex| myplex.account().clone())
    }

    pub fn server(&self) -> Option<PlexServer> {
        self.state.borrow().server.clone()
    }

    fn setting(&self, key: &str) -> Option<String> {
        Some(self.settings.string(key).to_string()).filter(|value| !value.is_empty())
    }

    fn set_setting(&self, key: &str, value: &str) {
        if let Err(err) = self.settings.set_string(key, value) {
            eprintln!("Could not save {key}: {err}");
        }
    }

    async fn token(&self, uuid: &str) -> Option<String> {
        match secrets::lookup(uuid).await {
            Ok(Some(token)) => return Some(token),
            Ok(None) => {}
            Err(err) => eprintln!("Could not read from the keyring: {err}"),
        }
        // The Python version wrote tokens to its config file when no Secret
        // Service was available. Such a token moves to the keyring as soon
        // as that works, and is then removed from the file.
        let token = store::legacy_token(uuid)?;
        let attributes = vec![("uuid", uuid.to_owned())];
        match secrets::store("Girens token", attributes, token.clone()).await {
            Ok(()) => store::remove_legacy_token(uuid),
            Err(err) => eprintln!("Could not move a token into the keyring: {err}"),
        }
        Some(token)
    }

    /// URL of the server that was used last, for status messages.
    pub fn stored_server_url(&self) -> Option<String> {
        self.setting("server-uuid")?;
        self.setting("server-url")
    }

    /// Signs in with the account token of an earlier session. `Ok(false)`
    /// means there is none.
    pub async fn restore_account(&self) -> Result<bool> {
        let Some(uuid) = self.setting("user-uuid") else {
            return Ok(false);
        };
        let Some(token) = self.token(&uuid).await else {
            return Ok(false);
        };
        let client = self.client.clone();
        let myplex = runtime::run(async move { MyPlex::with_token(&token, &client).await }).await?;
        self.set_account(myplex).await;
        Ok(true)
    }

    pub async fn login(&self, login: String, password: String) -> Result<()> {
        let client = self.client.clone();
        let myplex =
            runtime::run(async move { MyPlex::sign_in(&login, &password, &client).await }).await?;
        self.set_account(myplex).await;
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
                self.set_account(myplex).await;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn set_account(&self, myplex: MyPlex) {
        let account = myplex.account().clone();
        self.state.borrow_mut().account = Some(myplex);
        self.set_setting("user-uuid", &account.uuid);
        let attributes = vec![
            ("username", account.username.unwrap_or_default()),
            ("email", account.email.unwrap_or_default()),
            ("uuid", account.uuid),
        ];
        if let Err(err) = secrets::store("Girens token", attributes, account.auth_token).await {
            eprintln!("Could not store the account token in the keyring: {err}");
        }
    }

    /// Connects to the server of the previous session. `Ok(false)` means none
    /// was stored.
    pub async fn connect_stored_server(&self) -> Result<bool> {
        let (Some(uuid), Some(url)) = (self.setting("server-uuid"), self.setting("server-url"))
        else {
            return Ok(false);
        };
        let Some(token) = self.token(&uuid).await else {
            return Ok(false);
        };
        self.connect(url, token).await?;
        Ok(true)
    }

    /// Connects to a server by address, without a plex.tv account. As in the
    /// Python version this replaces an account login.
    pub async fn connect_url(&self, url: String, token: String) -> Result<()> {
        self.connect(url, token).await?;
        if let Some(uuid) = self.setting("user-uuid") {
            if let Err(err) = secrets::clear(&uuid).await {
                eprintln!("Could not remove the account token from the keyring: {err}");
            }
        }
        self.set_setting("user-uuid", "");
        self.state.borrow_mut().account = None;
        Ok(())
    }

    async fn connect(&self, url: String, token: String) -> Result<()> {
        let client = self.client.clone();
        let server =
            runtime::run(async move { PlexServer::connect(&url, &token, &client).await }).await?;
        self.set_server(server).await;
        Ok(())
    }

    /// The servers of the account, or only the connected one without account.
    pub async fn servers(&self) -> Result<Vec<Resource>> {
        let Some(myplex) = self.state.borrow().account.clone() else {
            return Ok(Vec::new());
        };
        runtime::run(async move { myplex.servers().await }).await
    }

    pub async fn connect_resource(&self, resource: &Resource) -> Result<()> {
        let Some(myplex) = self.state.borrow().account.clone() else {
            return Err(Error::Unauthorized);
        };
        let resource = resource.clone();
        let server = runtime::run(async move { myplex.connect(&resource).await }).await?;
        self.set_server(server).await;
        Ok(())
    }

    async fn set_server(&self, server: PlexServer) {
        let url = server.base_url().as_str().trim_end_matches('/').to_owned();
        self.set_setting("server-url", &url);
        self.set_setting("server-uuid", server.machine_identifier());
        let attributes = vec![
            (
                "name",
                server.friendly_name().unwrap_or_default().to_owned(),
            ),
            ("url", url),
            ("uuid", server.machine_identifier().to_owned()),
        ];
        let token = server.token().to_owned();
        self.state.borrow_mut().server = Some(server);
        if let Err(err) = secrets::store("Girens server token", attributes, token).await {
            eprintln!("Could not store the server token in the keyring: {err}");
        }
    }

    /// Forgets the account, the server and their tokens.
    pub async fn logout(&self) {
        for key in ["server-uuid", "user-uuid"] {
            if let Some(uuid) = self.setting(key) {
                if let Err(err) = secrets::clear(&uuid).await {
                    eprintln!("Could not remove a token from the keyring: {err}");
                }
            }
        }
        for key in ["server-url", "server-uuid", "user-uuid"] {
            self.set_setting(key, "");
        }
        let config = config_file();
        if config.exists() {
            if let Err(err) = std::fs::remove_file(&config) {
                eprintln!("Could not remove {}: {err}", config.display());
            }
        }
        *self.state.borrow_mut() = State::default();
    }
}

/// `$XDG_CONFIG_HOME/config`, the JSON file of the Python version.
pub fn config_file() -> std::path::PathBuf {
    glib::user_config_dir().join("config")
}
