//! Play queues: the server-side list of what plays next.

use reqwest::Method;
use serde::Deserialize;

use crate::model::{Item, ItemKind, LibrarySection};
use crate::server::{Envelope, LIBRARY_IDENTIFIER};
use crate::{PlexServer, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayQueue {
    #[serde(rename = "playQueueID")]
    pub id: u64,
    /// `playQueueItemID` of the item to start with.
    #[serde(rename = "playQueueSelectedItemID")]
    pub selected_item_id: Option<u64>,
    #[serde(default, rename = "playQueueShuffled")]
    pub shuffled: bool,
    #[serde(default, rename = "Metadata")]
    pub items: Vec<Item>,
}

impl PlayQueue {
    /// Id of a queue that only exists in this application.
    pub const LOCAL: u64 = 0;

    /// A queue the server does not know about, for playing offline copies.
    pub fn local(items: Vec<Item>) -> Self {
        Self {
            id: Self::LOCAL,
            selected_item_id: None,
            shuffled: false,
            items,
        }
    }

    /// Whether the server keeps this queue.
    pub fn is_local(&self) -> bool {
        self.id == Self::LOCAL
    }

    /// Position of the selected item in `items`.
    pub fn selected_index(&self) -> Option<usize> {
        let selected = self.selected_item_id?;
        self.items
            .iter()
            .position(|item| item.play_queue_item_id == Some(selected))
    }
}

/// `video` or `audio`, as play queues want it.
fn list_type(kind: ItemKind) -> &'static str {
    if kind.is_audio() {
        "audio"
    } else {
        "video"
    }
}

impl PlexServer {
    fn item_uri(&self, path: &str) -> String {
        format!(
            "server://{}/{}{}",
            self.machine_identifier(),
            LIBRARY_IDENTIFIER,
            path
        )
    }

    async fn play_queue(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<PlayQueue> {
        let envelope: Envelope<PlayQueue> = self.request(method, path, params).await?;
        Ok(envelope.container)
    }

    /// Queue for `item` (anything but a track or a whole section). With
    /// `continuous`, a show continues with the next episodes.
    pub async fn create_play_queue(
        &self,
        item: &Item,
        start_item: Option<&Item>,
        shuffle: bool,
    ) -> Result<PlayQueue> {
        let uri = self.item_uri(&item.key);
        let playlist_id = item.rating_key.to_string();
        let mut params = vec![
            ("includeChapters", "1"),
            ("includeRelated", "1"),
            ("repeat", "0"),
            ("shuffle", if shuffle { "1" } else { "0" }),
            ("continuous", "1"),
            ("uri", uri.as_str()),
        ];
        if item.kind == ItemKind::Playlist {
            params.push(("type", item.playlist_type.as_deref().unwrap_or("video")));
            params.push(("playlistID", &playlist_id));
        } else {
            params.push(("type", list_type(item.kind)));
        }
        if let Some(start_item) = start_item {
            params.push(("key", &start_item.key));
        }
        self.play_queue(Method::POST, "/playQueues", &params).await
    }

    /// Queue with everything in a library section, in the given sort order.
    pub async fn create_section_play_queue(
        &self,
        section: &LibrarySection,
        sort: Option<&str>,
        shuffle: bool,
    ) -> Result<PlayQueue> {
        let mut path = format!("/library/sections/{}/all", section.key);
        if let Some(sort) = sort {
            path.push_str("?sort=");
            path.push_str(sort);
        }
        let uri = self.item_uri(&path);
        let params = [
            ("type", list_type(section.kind)),
            ("uri", uri.as_str()),
            ("shuffle", if shuffle { "1" } else { "0" }),
            ("continuous", "0"),
            ("repeat", "0"),
            ("includeChapters", "1"),
            ("includeRelated", "1"),
        ];
        self.play_queue(Method::POST, "/playQueues", &params).await
    }

    /// Fetches the current state of a queue.
    pub async fn refresh_play_queue(&self, id: u64) -> Result<PlayQueue> {
        self.play_queue(Method::GET, &format!("/playQueues/{id}"), &[])
            .await
    }

    pub async fn shuffle_play_queue(&self, id: u64, shuffle: bool) -> Result<PlayQueue> {
        let action = if shuffle { "shuffle" } else { "unshuffle" };
        self.play_queue(Method::PUT, &format!("/playQueues/{id}/{action}"), &[])
            .await
    }
}
