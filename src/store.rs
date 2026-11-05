//! `$XDG_CONFIG_HOME/config`: the one-line JSON file in which the Python
//! version kept the sort order per library and the list of synced items.
//! The format is unchanged so that existing preferences carry over.

use serde_json::{json, Map, Value};

#[cfg(test)]
thread_local! {
    /// Lets a test use its own file.
    static TEST_FILE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
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

    #[test]
    fn the_python_config_is_understood() {
        with_config(|| {
            assert_eq!(
                section_sort("uuid-1"),
                Some(("addedAt".to_owned(), "desc".to_owned()))
            );
            assert_eq!(section_sort("unknown"), None);

            let mut entries = sync_entries();
            entries.sort_by_key(|entry| entry.rating_key);
            assert_eq!(
                entries,
                [
                    SyncEntry {
                        rating_key: 12,
                        converted: false,
                        only_unwatched: true,
                        max_items: Some(5),
                    },
                    SyncEntry {
                        rating_key: 34,
                        converted: true,
                        only_unwatched: false,
                        max_items: None,
                    },
                ]
            );
            assert_eq!(legacy_token("abc").as_deref(), Some("secret-token"));
        });
    }

    #[test]
    fn changing_one_part_keeps_the_others() {
        with_config(|| {
            set_section_sort("uuid-2", "titleSort", "asc");
            add_sync_entry(&SyncEntry {
                rating_key: 56,
                converted: false,
                only_unwatched: false,
                max_items: None,
            });
            remove_sync_entry(12);
            remove_legacy_token("abc");

            assert_eq!(
                section_sort("uuid-1"),
                Some(("addedAt".to_owned(), "desc".to_owned()))
            );
            assert_eq!(
                section_sort("uuid-2"),
                Some(("titleSort".to_owned(), "asc".to_owned()))
            );
            let mut keys: Vec<u64> = sync_entries().iter().map(|e| e.rating_key).collect();
            keys.sort_unstable();
            assert_eq!(keys, [34, 56]);
            assert_eq!(legacy_token("abc"), None);
            // The file stays one line of JSON, as the Python version read it.
            let text = std::fs::read_to_string(config_file()).unwrap();
            assert_eq!(text.lines().count(), 1);
            assert!(!text.contains("tokens"));
        });
    }

    #[test]
    fn an_existing_sync_entry_is_not_overwritten() {
        with_config(|| {
            add_sync_entry(&SyncEntry {
                rating_key: 12,
                converted: true,
                only_unwatched: false,
                max_items: None,
            });
            let entry = sync_entries()
                .into_iter()
                .find(|entry| entry.rating_key == 12)
                .unwrap();
            assert!(!entry.converted);
            assert_eq!(entry.max_items, Some(5));
        });
    }
}
