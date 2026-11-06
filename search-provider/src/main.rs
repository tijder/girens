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

fn load() -> Vec<Entry> {
    let path = xdg_dir("XDG_CONFIG_HOME", ".config").join("search_provider_data");
    std::fs::read_to_string(path)
        .map(|data| parse(&data))
        .unwrap_or_default()
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

fn launch(arguments: &[&str]) {
    if let Err(err) = Command::new("girens").args(arguments).spawn() {
        eprintln!("Could not start Girens: {err}");
    }
}

struct Provider;

#[zbus::interface(name = "org.gnome.Shell.SearchProvider2")]
impl Provider {
    fn get_initial_result_set(&self, terms: Vec<String>) -> Vec<String> {
        search(&load(), &terms)
    }

    fn get_subsearch_result_set(
        &self,
        _previous_results: Vec<String>,
        terms: Vec<String>,
    ) -> Vec<String> {
        search(&load(), &terms)
    }

    fn get_result_metas(&self, identifiers: Vec<String>) -> Vec<HashMap<String, OwnedVariant>> {
        let entries = load();
        let cache = xdg_dir("XDG_CACHE_HOME", ".cache");
        identifiers
            .iter()
            .filter_map(|id| {
                let entry = entries.iter().find(|entry| &entry.id == id)?;
                let (server, key) = id.split_once(';')?;
                // The cover Girens has cached for the item, if any.
                let dir = cache.join(server);
                let icon = [format!("thumb_{key}@2"), format!("thumb_{key}")]
                    .into_iter()
                    .map(|name| dir.join(name))
                    .find(|path| path.exists())
                    .unwrap_or_else(|| dir.join(format!("thumb_{key}")));
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
            .serve_at(OBJECT_PATH, Provider)
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
    fn broken_data_gives_no_results() {
        assert!(parse("").is_empty());
        assert!(parse("not json").is_empty());
        assert!(parse("{}").is_empty());
    }
}
