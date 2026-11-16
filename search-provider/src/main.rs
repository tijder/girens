//! GNOME Shell search provider for Girens.
//!
//! Searches the list of shows, artists and albums that Girens writes to
//! `$XDG_CONFIG_HOME/search_provider_data` and opens a result with
//! `girens --show-id <server>;<rating key>`. It never talks to the server.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use zbus::{OwnedVariant, Variant};

const BUS_NAME: &str = "nl.g4d.Girens.SearchProvider";
const OBJECT_PATH: &str = "/nl/g4d/GirensSearchProvider";

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback)
        })
}

/// One searchable entry of the data file.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    /// `<server machine identifier>;<rating key>`
    id: String,
    title: String,
    /// `show`, `artist` or `album`.
    kind: String,
}

/// Numbers were written as numbers by some versions and as text by others.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse(data: &str) -> Vec<Entry> {
    let Ok(data) = serde_json::from_str::<Value>(data.lines().next().unwrap_or_default()) else {
        return Vec::new();
    };
    let Some(sections) = data.get("sections").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for section in sections.values() {
        let Some(server) = section.get("server_machine_identifier").and_then(text) else {
            continue;
        };
        for item in section
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(title), Some(key)) = (
                item.get("title").and_then(Value::as_str),
                item.get("ratingKey").and_then(text),
            ) else {
                continue;
            };
            entries.push(Entry {
                id: format!("{server};{key}"),
                title: title.to_owned(),
                kind: item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
    }
    entries
}

fn data_file() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("search_provider_data")
}

/// What tells one version of a file from the next: when it was written and
/// how long it is.
type Version = (std::time::SystemTime, u64);

fn version(path: &std::path::Path) -> Option<Version> {
    let file = std::fs::metadata(path).ok()?;
    Some((file.modified().ok()?, file.len()))
}

/// The entries of the data file, read again only when Girens wrote a new
/// one: a search asks for them with every letter that is typed.
#[derive(Default)]
struct Entries {
    read: std::sync::Mutex<Option<(Version, std::sync::Arc<Vec<Entry>>)>>,
}

impl Entries {
    fn of(&self, path: &std::path::Path) -> std::sync::Arc<Vec<Entry>> {
        let Some(now) = version(path) else {
            return Default::default();
        };
        let mut read = self.read.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, entries)) = read.as_ref().filter(|(version, _)| *version == now) {
            return entries.clone();
        }
        let entries = std::sync::Arc::new(
            std::fs::read_to_string(path)
                .map(|data| parse(&data))
                .unwrap_or_default(),
        );
        *read = Some((now, entries.clone()));
        entries
    }

    fn load(&self) -> std::sync::Arc<Vec<Entry>> {
        self.of(&data_file())
    }
}

/// Ids of the entries whose title contains the search terms.
fn search(entries: &[Entry], terms: &[String]) -> Vec<String> {
    let query = terms.join(" ").to_lowercase();
    if query.trim().is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|entry| entry.title.to_lowercase().contains(&query))
        .map(|entry| entry.id.clone())
        .collect()
}

/// The newest cover Girens keeps in `dir` for the item `key`. Girens names
/// them `thumb_<key>~<picture>`, with `@2` behind it for a sharp one;
/// earlier versions left the picture out.
fn cover(dir: &std::path::Path, key: &str) -> Option<PathBuf> {
    let start = format!("thumb_{key}");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|file| {
            let name = file.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix(&start).is_some_and(|rest| {
                (rest.is_empty() || rest.starts_with(['~', '@'])) && !rest.ends_with(".part")
            })
        })
        .filter_map(|file| Some((file.metadata().ok()?.modified().ok()?, file.path())))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn launch(arguments: &[&str]) {
    if let Err(err) = Command::new("girens").args(arguments).spawn() {
        eprintln!("Could not start Girens: {err}");
    }
}

#[derive(Default)]
struct Provider {
    entries: Entries,
}

#[zbus::interface(name = "org.gnome.Shell.SearchProvider2")]
impl Provider {
    fn get_initial_result_set(&self, terms: Vec<String>) -> Vec<String> {
        search(&self.entries.load(), &terms)
    }

    fn get_subsearch_result_set(
        &self,
        _previous_results: Vec<String>,
        terms: Vec<String>,
    ) -> Vec<String> {
        search(&self.entries.load(), &terms)
    }

    fn get_result_metas(&self, identifiers: Vec<String>) -> Vec<HashMap<String, OwnedVariant>> {
        let entries = self.entries.load();
        let cache = xdg_dir("XDG_CACHE_HOME", ".cache");
        identifiers
            .iter()
            .filter_map(|id| {
                let entry = entries.iter().find(|entry| &entry.id == id)?;
                let (server, key) = id.split_once(';')?;
                // The cover Girens has cached for the item, if any.
                let dir = cache.join(server);
                let icon = cover(&dir, key).unwrap_or_else(|| dir.join(format!("thumb_{key}")));
                let mut meta = HashMap::new();
                let mut insert = |name: &str, value: String| {
                    if let Ok(value) = OwnedVariant::try_from(Variant::from(value)) {
                        meta.insert(name.to_owned(), value);
                    }
                };
                insert("id", id.clone());
                insert("name", entry.title.clone());
                insert("description", entry.kind.clone());
                insert("gicon", icon.to_string_lossy().into_owned());
                Some(meta)
            })
            .collect()
    }

    fn activate_result(&self, identifier: String, _terms: Vec<String>, _timestamp: u32) {
        launch(&["--show-id", &identifier]);
    }

    fn launch_search(&self, terms: Vec<String>, _timestamp: u32) {
        launch(&["--search", &terms.join(" ")]);
    }
}

fn main() -> zbus::Result<()> {
    zbus::block_on(async {
        let _connection = zbus::connection::Builder::session()
            .name(BUS_NAME)
            .serve_at(OBJECT_PATH, Provider::default())
            .build()
            .await?;
        // Requests are served for as long as the connection lives.
        std::future::pending::<()>().await;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = r#"{"sections": {"uuid-1": {"key": 6, "server_machine_identifier": "abc", "title": "Shows", "items": [{"title": "The Example Show", "titleSort": "Example Show", "ratingKey": 12, "type": "show"}, {"title": "Another", "titleSort": "Another", "ratingKey": "34", "type": "album"}]}}}"#;

    #[test]
    fn entries_are_read_with_numeric_or_text_keys() {
        let entries = parse(DATA);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "abc;12");
        assert_eq!(entries[1].id, "abc;34");
        assert_eq!(entries[1].kind, "album");
    }

    #[test]
    fn search_ignores_case_and_empty_queries() {
        let entries = parse(DATA);
        assert_eq!(
            search(&entries, &["example".into(), "show".into()]),
            ["abc;12"]
        );
        assert!(search(&entries, &["missing".into()]).is_empty());
        assert!(search(&entries, &[String::new()]).is_empty());
    }

    #[test]
    fn the_cover_of_an_item_is_found_whatever_picture_it_is() {
        let dir = std::env::temp_dir().join(format!("girens-sp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["thumb_123~aa", "thumb_1.part", "item_12"] {
            std::fs::write(dir.join(name), "picture").unwrap();
        }
        assert_eq!(cover(&dir, "12"), None);
        std::fs::write(dir.join("thumb_12~0123456789abcdef@2"), "picture").unwrap();
        assert_eq!(cover(&dir, "12"), Some(dir.join("thumb_12~0123456789abcdef@2")));
        assert_eq!(cover(&dir, "123"), Some(dir.join("thumb_123~aa")));
        assert_eq!(cover(&dir, "1"), None);
        // As earlier versions named them.
        std::fs::write(dir.join("thumb_1"), "picture").unwrap();
        assert_eq!(cover(&dir, "1"), Some(dir.join("thumb_1")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_data_is_read_again_only_when_it_changed() {
        let path = std::env::temp_dir().join(format!("girens-sp-data-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let entries = Entries::default();
        assert!(entries.of(&path).is_empty());

        std::fs::write(&path, DATA).unwrap();
        let first = entries.of(&path);
        assert_eq!(first.len(), 2);
        // The same list, not another reading of the file.
        assert!(std::sync::Arc::ptr_eq(&first, &entries.of(&path)));

        std::fs::write(&path, r#"{"sections": {}}"#).unwrap();
        assert!(entries.of(&path).is_empty());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn broken_data_gives_no_results() {
        assert!(parse("").is_empty());
        assert!(parse("not json").is_empty());
        assert!(parse("{}").is_empty());
    }
}
