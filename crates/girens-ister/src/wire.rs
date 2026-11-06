//! The answers of the server, as they arrive.

use serde::Deserialize;

/// A list that the server may also leave out or send as `null`.
fn list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub serving_node: Node,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub selected: bool,
    pub directory: Directory,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Metadata {
    pub language: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub released: Option<String>,
    pub genre: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchStatus {
    #[serde(default)]
    pub watched: bool,
    #[serde(default)]
    pub progress_in_milliseconds: u64,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFileStream {
    pub id: String,
    pub stream_index: Option<u32>,
    #[serde(default)]
    pub codec_name: String,
    #[serde(default)]
    pub codec_type: String,
    #[serde(default)]
    pub height: u32,
    pub language: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub id: String,
    pub size: Option<f64>,
    pub duration_in_milliseconds: Option<u64>,
    pub format: Option<String>,
    pub directory: Directory,
    #[serde(default, deserialize_with = "list")]
    pub media_file_streams: Vec<Option<MediaFileStream>>,
    #[serde(default, deserialize_with = "list")]
    pub segments: Vec<MediaSegment>,
}

/// An intro or the credits, in the time of the file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSegment {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub start_in_milliseconds: f64,
    #[serde(default)]
    pub end_in_milliseconds: f64,
    /// Whose it is, in a file that holds several episodes.
    pub episode_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFilePart {
    #[serde(default)]
    pub start_in_milliseconds: f64,
    #[serde(default)]
    pub duration_in_milliseconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Movie {
    pub id: String,
    pub name: String,
    pub release_year: Option<u32>,
    pub rating: Option<u32>,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
    #[serde(default, deserialize_with = "list")]
    pub metadata: Vec<Metadata>,
    #[serde(default, deserialize_with = "list")]
    pub watch_status: Vec<WatchStatus>,
    #[serde(default, deserialize_with = "list")]
    pub media_file: Vec<MediaFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchedFlag {
    #[serde(default)]
    pub watched: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeBrief {
    #[serde(default, deserialize_with = "list")]
    pub watch_status: Vec<WatchedFlag>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonBrief {
    pub id: String,
    pub number: u32,
    #[serde(default, deserialize_with = "list")]
    pub episodes: Vec<EpisodeBrief>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Show {
    pub id: String,
    pub name: String,
    pub release_year: Option<u32>,
    pub rating: Option<u32>,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
    #[serde(default, deserialize_with = "list")]
    pub metadata: Vec<Metadata>,
    /// Only asked for with a single show.
    pub seasons: Option<Vec<SeasonBrief>>,
}

/// The show an episode belongs to, as far as an episode tells.
#[derive(Debug, Clone, Deserialize)]
pub struct ShowRef {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonRef {
    pub id: String,
    pub number: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub number: u32,
    pub rating: Option<u32>,
    pub season: Option<SeasonRef>,
    pub show: Option<ShowRef>,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
    #[serde(default, deserialize_with = "list")]
    pub metadata: Vec<Metadata>,
    #[serde(default, deserialize_with = "list")]
    pub watch_status: Vec<WatchStatus>,
    #[serde(default, deserialize_with = "list")]
    pub media_file: Vec<MediaFile>,
    #[serde(default, deserialize_with = "list")]
    pub media_file_parts: Vec<MediaFilePart>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Season {
    pub id: String,
    pub number: u32,
    pub show: Show,
    #[serde(default, deserialize_with = "list")]
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShowEpisodes {
    #[serde(default, deserialize_with = "list")]
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
    #[serde(default, deserialize_with = "list")]
    pub metadata: Vec<Metadata>,
    /// Only asked for with a single artist.
    pub albums: Option<Vec<Album>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub name: String,
    pub release_year: Option<u32>,
    pub rating: Option<u32>,
    pub artist: PersonRef,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
    /// Only asked for with a single album.
    pub tracks: Option<Vec<Track>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumRef {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "list")]
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub number: u32,
    #[serde(default)]
    pub disc_number: u32,
    pub rating: Option<u32>,
    pub play_count: Option<u32>,
    pub artist: PersonRef,
    pub album: AlbumRef,
    #[serde(default, deserialize_with = "list")]
    pub metadata: Vec<Metadata>,
    #[serde(default, deserialize_with = "list")]
    pub media_file: Vec<MediaFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    #[serde(default)]
    pub total_elements: u32,
    #[serde(default = "Vec::new")]
    pub content: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Me {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    #[serde(default)]
    pub preferred_audio_languages: Vec<String>,
    #[serde(default)]
    pub preferred_subtitle_languages: Vec<String>,
    #[serde(default)]
    pub hide_subtitles_matching_audio: bool,
    #[serde(default)]
    pub auto_skip_intro: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamToken {
    pub token: String,
}

/// An entry of "continue watching", of a play queue or of a playlist: one of
/// the media fields is set.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// Absent for "continue watching".
    pub id: Option<String>,
    pub movie: Option<Movie>,
    pub episode: Option<Episode>,
    pub track: Option<Track>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayQueue {
    pub id: String,
    pub current_item_id: Option<String>,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default, deserialize_with = "list")]
    pub play_queue_items: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub library_type: String,
    pub item_count: Option<u32>,
    #[serde(default, deserialize_with = "list")]
    pub cover_images: Vec<Image>,
    /// Only asked for with a single playlist.
    pub items: Option<Vec<Entry>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "__typename")]
pub enum SearchResult {
    Movie(Movie),
    Show(Show),
    Episode(Episode),
    Person(Person),
    Album(Album),
    Track(Track),
    /// Books, podcasts and games, which Girens does not show.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Id {
    pub id: String,
}
