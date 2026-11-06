//! `$XDG_CONFIG_HOME/config`: one line of JSON with the accounts that are
//! signed in and, per user, the sort order of each library and the items to
//! keep offline.
//!
//! ```json
//! {"accounts": {"<account id>": {"kind": "plex", "name": "…", "profile": "<uuid>",
//!                                "profiles": […], "server": {"uuid": "…", "url": "…"}}},
//!  "users": {"<user key>": {"sections": {"<library uuid>": {"sort": "…", "sort_value": "…"}},
//!                           "servers": {"<machine id>": {"sync": {"<rating key>": {…}}}}}}}
//! ```
//!
//! An account is a plex.tv account or a server that was entered by address.
//! A user is a profile of the Plex Home of an account; its key is the uuid
//! of the profile, or the id of the account when that is a server.
//!
//! The Python version, and the Rust version before it knew more than one
//! user, kept `sections` and `sync` at the top; [`migrate`] hands those to
//! the user who was signed in.

use girens_plex::ItemId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[cfg(test)]
thread_local! {
    /// Lets a test use its own file.
    static TEST_FILE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Makes the calling test use `path` as the config file, or the real one
/// again with `None`.
#[cfg(test)]
pub fn use_test_file(path: Option<std::path::PathBuf>) {
    TEST_FILE.set(path);
}

fn config_file() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(path) = TEST_FILE.with_borrow(Clone::clone) {
        return path;
    }
    crate::session::config_file()
}

fn read() -> Map<String, Value> {
    std::fs::read_to_string(config_file())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text.lines().next()?).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

fn write(config: &Map<String, Value>) {
    let path = config_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = Value::Object(config.clone()).to_string();
    if let Err(err) = std::fs::write(&path, text) {
        eprintln!("Could not write {}: {err}", path.display());
    }
}

/// The object at `path`, created along the way.
fn object_mut<'a>(config: &'a mut Map<String, Value>, path: &[&str]) -> &'a mut Map<String, Value> {
    let mut current = config;
    for key in path {
        let entry = current
            .entry(key.to_string())
            .or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        current = entry.as_object_mut().expect("just made an object");
    }
    current
}

/// The object at `path`, if it is there.
fn object<'a>(config: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Map<String, Value>> {
    let mut current = config;
    for key in path {
        current = current.get(*key)?.as_object()?;
    }
    Some(current)
}

// Accounts

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    /// Signed in on plex.tv.
    #[default]
    Plex,
    /// A server entered by address and token.
    Server,
}

/// A profile of a Plex Home, as far as needed to offer it without plex.tv.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub uuid: String,
    pub title: String,
    pub thumb: Option<String>,
    pub protected: bool,
    pub restricted: bool,
    pub admin: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StoredServer {
    pub uuid: String,
    pub url: String,
}

/// An account that is signed in.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    /// The uuid on plex.tv, or `server:<machine id>`.
    #[serde(skip)]
    pub id: String,
    pub kind: AccountKind,
    pub name: String,
    pub thumb: Option<String>,
    /// The profile in use; the account itself when there is none.
    pub profile: Option<String>,
    /// The profiles of the Home, as plex.tv listed them last.
    pub profiles: Vec<Profile>,
    /// The server used last.
    pub server: Option<StoredServer>,
    /// What the keyring holds for this account, to remove when signing out.
    pub secrets: Vec<String>,
}

impl Account {
    /// The key of the user this account is used as.
    pub fn user_key(&self) -> &str {
        self.profile.as_deref().unwrap_or(&self.id)
    }

    /// The keys of everyone who can use this account.
    pub fn user_keys(&self) -> Vec<String> {
        let mut keys = vec![self.id.clone()];
        keys.extend(self.profile.clone());
        keys.extend(self.profiles.iter().map(|profile| profile.uuid.clone()));
        keys.sort();
        keys.dedup();
        keys
    }
}

/// Id of the account of a server that was entered by address.
pub fn server_account_id(machine_identifier: &str) -> String {
    format!("server:{machine_identifier}")
}

fn accounts_in(config: &Map<String, Value>) -> Vec<Account> {
    let Some(accounts) = object(config, &["accounts"]) else {
        return Vec::new();
    };
    let mut accounts: Vec<Account> = accounts
        .iter()
        .filter_map(|(id, value)| {
            let mut account: Account = serde_json::from_value(value.clone()).ok()?;
            account.id.clone_from(id);
            Some(account)
        })
        .collect();
    accounts.sort_by_key(|account| account.name.to_lowercase());
    accounts
}

pub fn accounts() -> Vec<Account> {
    accounts_in(&read())
}

pub fn account(id: &str) -> Option<Account> {
    accounts().into_iter().find(|account| account.id == id)
}

pub fn save_account(account: &Account) {
    let mut config = read();
    if let Ok(value) = serde_json::to_value(account) {
        object_mut(&mut config, &["accounts"]).insert(account.id.clone(), value);
        write(&config);
    }
}

/// Changes the stored account `id`, if there is one.
pub fn update_account(id: &str, change: impl FnOnce(&mut Account)) {
    if let Some(mut account) = account(id) {
        change(&mut account);
        save_account(&account);
    }
}

/// Forgets the account and what was kept for its users. Returns the keys of
/// the users that are gone; someone who is also known through another
/// account stays.
pub fn remove_account(id: &str) -> Vec<String> {
    let mut config = read();
    let accounts = accounts_in(&config);
    let Some(account) = accounts.iter().find(|account| account.id == id) else {
        return Vec::new();
    };
    let kept: Vec<String> = accounts
        .iter()
        .filter(|other| other.id != id)
        .flat_map(Account::user_keys)
        .collect();
    let gone: Vec<String> = account
        .user_keys()
        .into_iter()
        .filter(|key| !kept.contains(key))
        .collect();
    object_mut(&mut config, &["accounts"]).remove(id);
    if let Some(Value::Object(users)) = config.get_mut("users") {
        for key in &gone {
            users.remove(key);
        }
    }
    write(&config);
    gone
}

// Migration

/// The owner of what was stored before there were several users.
#[derive(Debug, Clone, PartialEq)]
pub struct Migration {
    pub account: String,
    pub user: String,
    /// The server that was in use, whose token and downloads become this
    /// user's.
    pub server: Option<String>,
}

fn migration_in(config: &Map<String, Value>) -> Option<Migration> {
    let pending = object(config, &["migrating"])?;
    let text = |key: &str| pending.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(Migration {
        account: text("account")?,
        user: text("user")?,
        server: text("server"),
    })
}

/// Hands what the single user of earlier versions had to an account.
/// `user_uuid` and `server` are what the settings remembered. Returns what
/// is left to move outside this file (the keyring, the downloads), until
/// [`finish_migration`] is called.
pub fn migrate(user_uuid: Option<&str>, server: Option<&StoredServer>) -> Option<Migration> {
    let mut config = read();
    if config.contains_key("accounts") {
        return migration_in(&config);
    }
    let account = match (user_uuid, server) {
        (Some(uuid), _) => Account {
            id: uuid.to_owned(),
            kind: AccountKind::Plex,
            server: server.cloned(),
            ..Account::default()
        },
        (None, Some(server)) => Account {
            id: server_account_id(&server.uuid),
            kind: AccountKind::Server,
            name: server.url.clone(),
            server: Some(server.clone()),
            ..Account::default()
        },
        // Nobody was signed in, so there is nothing that belongs to anyone.
        (None, None) => return None,
    };
    let user = account.user_key().to_owned();
    if let Some(sections) = config.remove("sections") {
        object_mut(&mut config, &["users", &user]).insert("sections".into(), sections);
    }
    if let Some(sync) = config.remove("sync") {
        // The list did not say which server its items are on; it was the
        // one in use.
        if let Some(server) = server {
            object_mut(&mut config, &["users", &user, "servers", &server.uuid])
                .insert("sync".into(), sync);
        }
    }
    let mut pending = json!({ "account": account.id, "user": user });
    if let Some(server) = server {
        pending["server"] = json!(server.uuid);
    }
    config.insert("migrating".into(), pending);
    if let Ok(value) = serde_json::to_value(&account) {
        object_mut(&mut config, &["accounts"]).insert(account.id.clone(), value);
    }
    write(&config);
    migration_in(&config)
}

pub fn finish_migration() {
    let mut config = read();
    if config.remove("migrating").is_some() {
        write(&config);
    }
}

// Sort orders

/// Sort key and direction (`asc`/`desc`) the user used last for a library.
pub fn section_sort(user: &str, section_uuid: &str) -> Option<(String, String)> {
    let config = read();
    let entry = object(&config, &["users", user, "sections", section_uuid])?;
    Some((
        entry.get("sort")?.as_str()?.to_owned(),
        entry.get("sort_value")?.as_str()?.to_owned(),
    ))
}

pub fn set_section_sort(user: &str, section_uuid: &str, sort: &str, direction: &str) {
    let mut config = read();
    object_mut(&mut config, &["users", user, "sections"]).insert(
        section_uuid.to_owned(),
        json!({ "sort": sort, "sort_value": direction }),
    );
    write(&config);
}

// Offline copies

/// An item the user wants to have available offline.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncEntry {
    pub rating_key: ItemId,
    /// Download a transcoded version instead of the original file.
    pub converted: bool,
    pub only_unwatched: bool,
    pub max_items: Option<u32>,
}

/// Booleans and numbers were not always written with the same JSON type.
fn loose_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

/// What `user` wants to have offline of the server `machine_identifier`.
pub fn sync_entries(user: &str, machine_identifier: &str) -> Vec<SyncEntry> {
    let config = read();
    let Some(entries) = object(
        &config,
        &["users", user, "servers", machine_identifier, "sync"],
    ) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(key, entry)| {
            Some(SyncEntry {
                rating_key: key.parse().ok()?,
                converted: entry
                    .get("converted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                only_unwatched: entry
                    .get("only_unwatched")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                max_items: entry
                    .get("max_items")
                    .and_then(loose_u64)
                    .map(|max| max as u32),
            })
        })
        .collect()
}

/// Adds `entry` unless its item is already synced.
pub fn add_sync_entry(user: &str, machine_identifier: &str, entry: &SyncEntry) {
    let mut config = read();
    let entries = object_mut(
        &mut config,
        &["users", user, "servers", machine_identifier, "sync"],
    );
    let key = entry.rating_key.to_string();
    if entries.contains_key(&key) {
        return;
    }
    let mut value = json!({
        "rating_key": key,
        "converted": entry.converted,
        "only_unwatched": entry.only_unwatched,
    });
    if let Some(max_items) = entry.max_items {
        value["max_items"] = json!(max_items);
    }
    entries.insert(key, value);
    write(&config);
}

pub fn remove_sync_entry(user: &str, machine_identifier: &str, rating_key: ItemId) {
    let mut config = read();
    let entries = object_mut(
        &mut config,
        &["users", user, "servers", machine_identifier, "sync"],
    );
    if entries.remove(&rating_key.to_string()).is_some() {
        write(&config);
    }
}

/// A token the Python version wrote to the config file because no Secret
/// Service was available at the time.
pub fn legacy_token(uuid: &str) -> Option<String> {
    read().get("tokens")?.get(uuid)?.as_str().map(str::to_owned)
}

/// Removes such a token once it is safe in the keyring.
pub fn remove_legacy_token(uuid: &str) {
    let mut config = read();
    let Some(Value::Object(tokens)) = config.get_mut("tokens") else {
        return;
    };
    if tokens.remove(uuid).is_none() {
        return;
    }
    if tokens.is_empty() {
        config.remove("tokens");
    }
    write(&config);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config file as the Python version wrote it.
    const PYTHON_CONFIG: &str = r#"{"tokens": {"abc": "secret-token"}, "sections": {"uuid-1": {"sort": "addedAt", "sort_value": "desc"}}, "sync": {"12": {"rating_key": "12", "converted": false, "only_unwatched": true, "max_items": 5}, "34": {"rating_key": "34", "converted": true}}}"#;

    fn with_config(test: impl FnOnce()) {
        let path = std::env::temp_dir().join(format!(
            "girens-store-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, PYTHON_CONFIG).unwrap();
        TEST_FILE.set(Some(path.clone()));
        test();
        TEST_FILE.set(None);
        let _ = std::fs::remove_file(path);
    }

    fn server() -> StoredServer {
        StoredServer {
            uuid: "machine".into(),
            url: "https://plex.example.org".into(),
        }
    }

    fn entry(rating_key: ItemId) -> SyncEntry {
        SyncEntry {
            rating_key,
            converted: false,
            only_unwatched: false,
            max_items: None,
        }
    }

    #[test]
    fn the_single_user_of_before_gets_what_was_stored() {
        with_config(|| {
            let migration = migrate(Some("me"), Some(&server())).unwrap();
            assert_eq!(
                migration,
                Migration {
                    account: "me".into(),
                    user: "me".into(),
                    server: Some("machine".into()),
                }
            );

            let stored = account("me").unwrap();
            assert_eq!(stored.kind, AccountKind::Plex);
            assert_eq!(stored.user_key(), "me");
            assert_eq!(stored.server, Some(server()));

            assert_eq!(
                section_sort("me", "uuid-1"),
                Some(("addedAt".to_owned(), "desc".to_owned()))
            );
            assert_eq!(section_sort("me", "unknown"), None);
            assert_eq!(section_sort("someone else", "uuid-1"), None);

            let mut entries = sync_entries("me", "machine");
            entries.sort_by_key(|entry| entry.rating_key);
            assert_eq!(
                entries,
                [
                    SyncEntry {
                        rating_key: 12.into(),
                        converted: false,
                        only_unwatched: true,
                        max_items: Some(5),
                    },
                    SyncEntry {
                        rating_key: 34.into(),
                        converted: true,
                        only_unwatched: false,
                        max_items: None,
                    },
                ]
            );
            assert!(sync_entries("me", "another server").is_empty());
            assert!(sync_entries("someone else", "machine").is_empty());
            assert_eq!(legacy_token("abc").as_deref(), Some("secret-token"));

            // The rest of the move is asked for until it is done, and the
            // file is not migrated twice.
            assert_eq!(migrate(Some("other"), None), Some(migration));
            finish_migration();
            assert_eq!(migrate(Some("other"), None), None);
            assert!(account("other").is_none());
            let text = std::fs::read_to_string(config_file()).unwrap();
            assert_eq!(text.lines().count(), 1);
        });
    }

    #[test]
    fn a_server_without_account_is_migrated_too() {
        with_config(|| {
            let migration = migrate(None, Some(&server())).unwrap();
            assert_eq!(migration.account, "server:machine");
            assert_eq!(migration.user, "server:machine");
            assert_eq!(account("server:machine").unwrap().kind, AccountKind::Server);
            assert_eq!(sync_entries("server:machine", "machine").len(), 2);
        });
    }

    #[test]
    fn nothing_is_migrated_when_nobody_was_signed_in() {
        with_config(|| {
            assert_eq!(migrate(None, None), None);
            assert!(accounts().is_empty());
        });
    }

    #[test]
    fn every_user_has_an_own_sort_order_and_sync_list() {
        with_config(|| {
            migrate(Some("me"), Some(&server()));
            set_section_sort("child", "uuid-1", "titleSort", "asc");
            add_sync_entry("child", "machine", &entry(56.into()));
            add_sync_entry("me", "second server", &entry(78.into()));
            remove_sync_entry("me", "machine", 12.into());
            remove_sync_entry("child", "machine", 34.into());

            assert_eq!(
                section_sort("me", "uuid-1"),
                Some(("addedAt".to_owned(), "desc".to_owned()))
            );
            assert_eq!(
                section_sort("child", "uuid-1"),
                Some(("titleSort".to_owned(), "asc".to_owned()))
            );
            let keys = |user: &str, server: &str| {
                let mut keys: Vec<ItemId> = sync_entries(user, server)
                    .iter()
                    .map(|entry| entry.rating_key)
                    .collect();
                keys.sort_unstable();
                keys
            };
            assert_eq!(keys("me", "machine"), [ItemId::from(34)]);
            assert_eq!(keys("child", "machine"), [ItemId::from(56)]);
            assert_eq!(keys("me", "second server"), [ItemId::from(78)]);
        });
    }

    #[test]
    fn an_existing_sync_entry_is_not_overwritten() {
        with_config(|| {
            migrate(Some("me"), Some(&server()));
            add_sync_entry(
                "me",
                "machine",
                &SyncEntry {
                    converted: true,
                    ..entry(12.into())
                },
            );
            let entry = sync_entries("me", "machine")
                .into_iter()
                .find(|entry| entry.rating_key == 12.into())
                .unwrap();
            assert!(!entry.converted);
            assert_eq!(entry.max_items, Some(5));
        });
    }

    #[test]
    fn signing_out_removes_only_what_belongs_to_that_account() {
        with_config(|| {
            migrate(Some("me"), Some(&server()));
            update_account("me", |account| {
                account.profiles = vec![
                    Profile {
                        uuid: "me".into(),
                        ..Profile::default()
                    },
                    Profile {
                        uuid: "child".into(),
                        ..Profile::default()
                    },
                    Profile {
                        uuid: "partner".into(),
                        ..Profile::default()
                    },
                ];
                account.profile = Some("child".into());
            });
            assert_eq!(account("me").unwrap().user_key(), "child");
            // The partner is also signed in with an account of their own.
            save_account(&Account {
                id: "partner".into(),
                name: "Partner".into(),
                ..Account::default()
            });
            for user in ["me", "child", "partner"] {
                add_sync_entry(user, "machine", &entry(99.into()));
            }

            let mut gone = remove_account("me");
            gone.sort();
            assert_eq!(gone, ["child", "me"]);
            assert!(account("me").is_none());
            assert!(sync_entries("me", "machine").is_empty());
            assert!(sync_entries("child", "machine").is_empty());
            assert_eq!(sync_entries("partner", "machine").len(), 1);
            assert_eq!(accounts().len(), 1);
            // Old tokens of the Python version are not touched.
            assert_eq!(legacy_token("abc").as_deref(), Some("secret-token"));
        });
    }

    #[test]
    fn a_legacy_token_can_be_removed() {
        with_config(|| {
            remove_legacy_token("abc");
            assert_eq!(legacy_token("abc"), None);
            let text = std::fs::read_to_string(config_file()).unwrap();
            assert!(!text.contains("tokens"));
        });
    }
}
