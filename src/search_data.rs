//! Writes the list of shows, artists and albums that the GNOME Shell search
//! provider (`search-provider/`) searches. The format is the one the Python
//! version used.

use girens_plex::model::{Item, ItemKind};
use girens_plex::PlexServer;
use gtk::glib;
use serde_json::{json, Map, Value};

use crate::runtime;

const PAGE_SIZE: u32 = 500;

/// `$XDG_CONFIG_HOME/search_provider_data`
fn data_file() -> std::path::PathBuf {
    glib::user_config_dir().join("search_provider_data")
}

fn entry(item: &Item) -> Value {
    json!({
        "title": item.title,
        "titleSort": item.title_sort.as_deref().unwrap_or(&item.title),
        "ratingKey": item.rating_key,
        "type": item.kind.as_str(),
    })
}

async fn collect(server: &PlexServer) -> girens_plex::Result<Value> {
    let mut sections = Map::new();
    for section in server.sections().await? {
        // Movies are not offered: opening one would start playing it.
        if !matches!(section.kind, ItemKind::Show | ItemKind::Artist) {
            continue;
        }
        let mut items = Vec::new();
        let mut offset = 0;
        loop {
            let page = server
                .section_items(section.key, offset, PAGE_SIZE, None)
                .await?;
            items.extend(page.items.iter().map(entry));
            offset += PAGE_SIZE;
            if page.items.is_empty() || offset >= page.total {
                break;
            }
        }
        if section.kind == ItemKind::Artist {
            items.extend(server.section_albums(section.key).await?.iter().map(entry));
        }
        sections.insert(
            section.uuid.clone(),
            json!({
                "key": section.key,
                "server_machine_identifier": server.machine_identifier(),
                "title": section.title,
                "items": items,
            }),
        );
    }
    Ok(json!({ "sections": sections }))
}

/// How long a written file is considered up to date.
const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Whether the file was written recently for this server.
async fn is_fresh(server: &PlexServer) -> bool {
    let path = data_file();
    let recent = tokio::fs::metadata(&path)
        .await
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age < MAX_AGE));
    recent
        && tokio::fs::read_to_string(&path)
            .await
            .is_ok_and(|data| data.contains(server.machine_identifier()))
}

/// Rebuilds the file for `server` in the background, unless it is recent.
pub fn refresh(server: PlexServer) {
    glib::spawn_future_local(async move {
        let written = runtime::run(async move {
            if is_fresh(&server).await {
                return Ok(());
            }
            let data = collect(&server).await.map_err(|err| err.to_string())?;
            tokio::fs::write(data_file(), data.to_string())
                .await
                .map_err(|err| err.to_string())
        })
        .await;
        if let Err(err) = written {
            eprintln!("Could not update the search provider data: {err}");
        }
    });
}
