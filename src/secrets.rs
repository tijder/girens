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
    match gio::spawn_blocking(task).await {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// The token stored for an account or server uuid.
pub async fn lookup(uuid: &str) -> Result<Option<String>, glib::Error> {
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
    let uuid = uuid.to_owned();
    blocking(move || {
        let attributes = HashMap::from([("uuid", uuid.as_str())]);
        libsecret::password_clear_sync(Some(&schema(&["uuid"])), attributes, gio::Cancellable::NONE)
    })
    .await
}
