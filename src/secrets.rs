//! Plex tokens in the Secret Service, stored the way the Python version did
//! so that existing logins survive the upgrade: schema `nl.g4d.Girens`,
//! looked up by the `uuid` attribute.

use std::collections::HashMap;

use gtk::{gio, glib};
use libsecret::{Schema, SchemaAttributeType, SchemaFlags};

const SCHEMA_NAME: &str = "nl.g4d.Girens";

fn schema(attributes: &[&str]) -> Schema {
    Schema::new(
        SCHEMA_NAME,
        SchemaFlags::NONE,
        attributes
            .iter()
            .map(|name| (*name, SchemaAttributeType::String))
            .collect(),
    )
}

async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, glib::Error> + Send + 'static,
) -> Result<T, glib::Error> {
    // libsecret registers its types on first use and that is not safe from
    // two threads at once, so keyring calls take turns.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let task = move || {
        let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        task()
    };
    match gio::spawn_blocking(task).await {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
thread_local! {
    /// Stands in for the keyring under test: tokens by their `uuid`.
    pub static TEST_KEYRING: std::cell::RefCell<HashMap<String, String>> =
        std::cell::RefCell::default();
}

/// The token stored for an account or server uuid.
pub async fn lookup(uuid: &str) -> Result<Option<String>, glib::Error> {
    #[cfg(test)]
    if true {
        return Ok(TEST_KEYRING.with_borrow(|keyring| keyring.get(uuid).cloned()));
    }
    let uuid = uuid.to_owned();
    blocking(move || {
        let attributes = HashMap::from([("uuid", uuid.as_str())]);
        libsecret::password_lookup_sync(
            Some(&schema(&["uuid"])),
            attributes,
            gio::Cancellable::NONE,
        )
        .map(|token| token.map(Into::into))
    })
    .await
}

/// Stores `token` under `label`. `attributes` must include `uuid`.
pub async fn store(
    label: &'static str,
    attributes: Vec<(&'static str, String)>,
    token: String,
) -> Result<(), glib::Error> {
    #[cfg(test)]
    if true {
        let uuid = attributes.iter().find(|(name, _)| *name == "uuid");
        let uuid = uuid.expect("a token is stored by its uuid").1.clone();
        TEST_KEYRING.with_borrow_mut(|keyring| keyring.insert(uuid, token));
        let _ = label;
        return Ok(());
    }
    blocking(move || {
        let names: Vec<&str> = attributes.iter().map(|(name, _)| *name).collect();
        let values: HashMap<&str, &str> = attributes
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        libsecret::password_store_sync(
            Some(&schema(&names)),
            values,
            Some(libsecret::COLLECTION_DEFAULT),
            label,
            &token,
            gio::Cancellable::NONE,
        )
    })
    .await
}

pub async fn clear(uuid: &str) -> Result<(), glib::Error> {
    #[cfg(test)]
    if true {
        TEST_KEYRING.with_borrow_mut(|keyring| keyring.remove(uuid));
        return Ok(());
    }
    let uuid = uuid.to_owned();
    blocking(move || {
        let attributes = HashMap::from([("uuid", uuid.as_str())]);
        libsecret::password_clear_sync(Some(&schema(&["uuid"])), attributes, gio::Cancellable::NONE)
    })
    .await
}
