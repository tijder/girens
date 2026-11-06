//! Items, libraries and the files behind them.
//!
//! The names and the JSON form are those of Plex, which is also how
//! downloads are described on disk; other servers are mapped onto them.

use serde::{Deserialize, Serialize};

use crate::{de, ItemId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Movie,
    Show,
    Season,
    Episode,
    Artist,
    Album,
    Track,
    Playlist,
    Clip,
    Photo,
    #[default]
    #[serde(other)]
    Other,
}

impl ItemKind {
    /// The numeric type Plex uses in search and filter queries.
    pub fn search_type(self) -> Option<u32> {
        Some(match self {
            ItemKind::Movie => 1,
            ItemKind::Show => 2,
            ItemKind::Season => 3,
            ItemKind::Episode => 4,
            ItemKind::Artist => 8,
            ItemKind::Album => 9,
            ItemKind::Track => 10,
            ItemKind::Clip => 12,
            ItemKind::Photo => 13,
            ItemKind::Playlist => 15,
            ItemKind::Other => return None,
        })
    }

    /// The name Plex uses for the type.
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Movie => "movie",
            ItemKind::Show => "show",
            ItemKind::Season => "season",
            ItemKind::Episode => "episode",
            ItemKind::Artist => "artist",
            ItemKind::Album => "album",
            ItemKind::Track => "track",
            ItemKind::Playlist => "playlist",
            ItemKind::Clip => "clip",
            ItemKind::Photo => "photo",
            ItemKind::Other => "",
        }
    }

    /// Tracks and albums go through the audio transcoder, the rest is video.
    pub fn is_audio(self) -> bool {
        matches!(self, ItemKind::Track | ItemKind::Album | ItemKind::Artist)
    }

    /// Whether the item itself can be played (as opposed to opened).
    pub fn is_playable(self) -> bool {
        matches!(
            self,
            ItemKind::Movie | ItemKind::Episode | ItemKind::Track | ItemKind::Clip
        )
    }
}

/// A movie, show, season, episode, artist, album, track, playlist or clip.
///
/// Plex describes all of them with the same loosely filled record; `kind`
/// tells which fields are meaningful.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub rating_key: ItemId,
    pub key: String,
    #[serde(rename = "type")]
    pub kind: ItemKind,
    #[serde(default)]
    pub title: String,
    pub title_sort: Option<String>,
    pub summary: Option<String>,
    pub year: Option<u32>,
    pub index: Option<u32>,
    pub parent_index: Option<u32>,
    /// Milliseconds.
    pub duration: Option<u64>,
    /// Resume position in milliseconds.
    #[serde(default)]
    pub view_offset: u64,
    #[serde(default)]
    pub view_count: u32,
    /// 0–10; the UI shows half of it as stars.
    pub user_rating: Option<f32>,
    pub last_viewed_at: Option<i64>,
    pub originally_available_at: Option<String>,

    pub thumb: Option<String>,
    pub art: Option<String>,
    pub composite: Option<String>,
    pub parent_thumb: Option<String>,
    pub grandparent_thumb: Option<String>,

    pub parent_title: Option<String>,
    pub grandparent_title: Option<String>,
    #[serde(default)]
    pub parent_rating_key: Option<ItemId>,
    #[serde(default)]
    pub grandparent_rating_key: Option<ItemId>,
    #[serde(rename = "librarySectionID")]
    pub library_section_id: Option<ItemId>,

    /// Music video for a track or trailer for a movie.
    pub primary_extra_key: Option<String>,
    pub leaf_count: Option<u32>,
    pub viewed_leaf_count: Option<u32>,
    pub child_count: Option<u32>,
    pub playlist_type: Option<String>,
    #[serde(default, deserialize_with = "de::flag")]
    pub smart: bool,
    /// Only set for items that are part of a play queue.
    #[serde(rename = "playQueueItemID")]
    pub play_queue_item_id: Option<ItemId>,

    #[serde(default, rename = "Genre")]
    pub genres: Vec<Tag>,
    #[serde(default, rename = "Media")]
    pub media: Vec<Media>,
}

impl Item {
    /// Played at least once, or for shows, seasons and albums: completely.
    pub fn is_watched(&self) -> bool {
        match (self.leaf_count, self.viewed_leaf_count) {
            (Some(total), Some(viewed)) => total > 0 && viewed >= total,
            _ => self.view_count > 0,
        }
    }

    /// The image to show on a cover tile: playlists have a composite and
    /// episodes show the poster of their show.
    pub fn cover(&self) -> Option<&str> {
        match self.kind {
            ItemKind::Playlist => self.composite.as_deref().or(self.thumb.as_deref()),
            ItemKind::Episode => self.grandparent_thumb.as_deref().or(self.thumb.as_deref()),
            _ => self.thumb.as_deref(),
        }
    }

    /// `s01e02` for episodes.
    pub fn season_episode(&self) -> Option<String> {
        match (self.kind, self.parent_index, self.index) {
            (ItemKind::Episode, Some(season), Some(episode)) => {
                Some(format!("s{season:02}e{episode:02}"))
            }
            _ => None,
        }
    }

    /// Whether this is an album of a photo library rather than a photo:
    /// Plex calls both a photo, but only a picture has a file.
    pub fn is_photo_album(&self) -> bool {
        self.kind == ItemKind::Photo && self.media.is_empty()
    }

    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        self.media.iter().flat_map(|media| media.parts.iter())
    }

    pub fn streams(&self, kind: StreamKind) -> impl Iterator<Item = &Stream> {
        self.parts()
            .flat_map(|part| part.streams.iter())
            .filter(move |stream| stream.kind() == kind)
    }

    pub fn selected_stream(&self, kind: StreamKind) -> Option<&Stream> {
        self.streams(kind).find(|stream| stream.selected)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tag {
    pub tag: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub id: ItemId,
    pub container: Option<String>,
    pub video_resolution: Option<String>,
    #[serde(default, rename = "Part")]
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub id: ItemId,
    /// Path of the media file on the server, e.g. `/library/parts/1/2/file.mkv`.
    pub key: String,
    pub container: Option<String>,
    /// Bytes.
    pub size: Option<u64>,
    #[serde(default, rename = "Stream")]
    pub streams: Vec<Stream>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Other,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub id: ItemId,
    pub stream_type: u32,
    /// Position among the streams of the file; absent for external subtitles.
    pub index: Option<u32>,
    /// Set for external (sidecar) subtitles, which mpv has to load separately.
    pub key: Option<String>,
    pub codec: Option<String>,
    pub title: Option<String>,
    pub display_title: Option<String>,
    pub extended_display_title: Option<String>,
    pub language: Option<String>,
    pub language_code: Option<String>,
    #[serde(default)]
    pub selected: bool,
}

impl Stream {
    pub fn kind(&self) -> StreamKind {
        match self.stream_type {
            1 => StreamKind::Video,
            2 => StreamKind::Audio,
            3 => StreamKind::Subtitle,
            _ => StreamKind::Other,
        }
    }

    pub fn is_external(&self) -> bool {
        self.key.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySection {
    pub key: ItemId,
    pub uuid: String,
    pub title: String,
    /// `movie`, `show`, `artist` or `photo`.
    #[serde(rename = "type")]
    pub kind: ItemKind,
}

/// One entry of a section's sort menu.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortOption {
    pub key: String,
    pub title: String,
    pub default_direction: Option<String>,
}
