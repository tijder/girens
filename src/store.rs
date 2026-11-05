//! `$XDG_CONFIG_HOME/config`: the one-line JSON file in which the Python
//! version kept the sort order per library and the list of synced items.
//! The format is unchanged so that existing preferences carry over.

use serde_json::{json, Map, Value};

use crate::session::config_file;

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

/// Sort key and direction (`asc`/`desc`) last used for a library.
pub fn section_sort(section_uuid: &str) -> Option<(String, String)> {
    let config = read();
    let entry = config.get("sections")?.get(section_uuid)?;
    Some((
        entry.get("sort")?.as_str()?.to_owned(),
        entry.get("sort_value")?.as_str()?.to_owned(),
    ))
}

pub fn set_section_sort(section_uuid: &str, sort: &str, direction: &str) {
    let mut config = read();
    let sections = config.entry("sections").or_insert_with(|| json!({}));
    if let Value::Object(sections) = sections {
        sections.insert(
            section_uuid.to_owned(),
            json!({ "sort": sort, "sort_value": direction }),
        );
        write(&config);
    }
}

/// An item the user wants to have available offline.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncEntry {
    pub rating_key: u64,
    /// Download a transcoded version instead of the original file.
    pub converted: bool,
    pub only_unwatched: bool,
    pub max_items: Option<u32>,
}

/// Booleans and numbers were not always written with the same JSON type.
fn loose_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

pub fn sync_entries() -> Vec<SyncEntry> {
    let config = read();
    let Some(Value::Object(entries)) = config.get("sync") else {
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
pub fn add_sync_entry(entry: &SyncEntry) {
    let mut config = read();
    let entries = config.entry("sync").or_insert_with(|| json!({}));
    let Value::Object(entries) = entries else {
        return;
    };
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

pub fn remove_sync_entry(rating_key: u64) {
    let mut config = read();
    if let Some(Value::Object(entries)) = config.get_mut("sync") {
        if entries.remove(&rating_key.to_string()).is_some() {
            write(&config);
        }
    }
}
