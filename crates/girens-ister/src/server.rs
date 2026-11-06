//! A connection to an Ister server.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use girens_model::model::{Item, ItemKind, LibrarySection, Part, SortOption};
use girens_model::{Error, ItemId, Page, PlayQueue, PlaybackState, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Provider, Tokens, WellKnown};
use crate::map::{self, Context, Selection};
use crate::{queries, wire};

/// A token for media and images lasts a day; it is replaced well before.
const STREAM_TOKEN_AGE: Duration = Duration::from_secs(12 * 3600);
/// How long the results of a search are kept, for the next kind asked for.
const SEARCH_AGE: Duration = Duration::from_secs(30);
/// The widths the server keeps images in.
const IMAGE_WIDTHS: [u32; 7] = [160, 240, 320, 480, 640, 960, 1280];
/// How far the subtitle files of the server are shifted, in seconds.
pub const SUBTITLE_DELAY: f64 = -1.48;

/// Called with the new refresh token whenever the provider replaces it.
pub type TokenKeeper = Arc<dyn Fn(&str) + Send + Sync>;

/// Who is signed in.
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

/// What a play queue was made of, to make it again in another order.
#[derive(Clone)]
struct QueueSource {
    input: Value,
}

/// The settings a stream was opened with, which the server wants to hear
/// with every report.
#[derive(Clone, Copy)]
struct Opened {
    direct: bool,
    file: ItemId,
}

struct Inner {
    http: reqwest::Client,
    well_known: WellKnown,
    machine: String,
    provider: Provider,
    tokens: tokio::sync::Mutex<Tokens>,
    keeper: TokenKeeper,
    user: User,
    /// Only told to the server when it is a UUID, as the server wants.
    device: Option<String>,
    stream_token: RwLock<(String, Instant)>,
    stream_token_refresh: tokio::sync::Mutex<()>,
    context: RwLock<Context>,
    queues: Mutex<HashMap<ItemId, QueueSource>>,
    opened: Mutex<HashMap<ItemId, Opened>>,
    search: Mutex<Option<(String, Instant, Vec<Item>)>>,
    /// Where the answers of the server are written, to make test data.
    record: Mutex<Option<std::path::PathBuf>>,
}

#[derive(Clone)]
pub struct IsterServer {
    inner: Arc<Inner>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
struct Answer<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

/// A name for the server that can also name a folder: it has no id of its
/// own, so its address stands in.
pub fn machine_identifier(api_url: &str) -> String {
    let address = api_url
        .split_once("://")
        .map_or(api_url, |(_, address)| address);
    let safe: String = address
        .trim_end_matches('/')
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() || c == '.' || c == '-' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    format!("ister_{safe}")
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl IsterServer {
    /// Connects with tokens from a sign-in or from an earlier session
    /// (see [`auth::refresh`]). `languages` are those to show titles in,
    /// most wanted first; `device` names this player to the server.
    pub async fn connect(
        http: reqwest::Client,
        well_known: WellKnown,
        provider: Provider,
        tokens: Tokens,
        languages: Vec<String>,
        device: String,
        keeper: TokenKeeper,
    ) -> Result<Self> {
        let mut server = Self {
            inner: Arc::new(Inner {
                http,
                machine: machine_identifier(&well_known.api_url),
                well_known,
                provider,
                tokens: tokio::sync::Mutex::new(tokens),
                keeper,
                user: User {
                    id: String::new(),
                    name: String::new(),
                    email: None,
                },
                device: Some(device).filter(|device| is_uuid(device)),
                stream_token: RwLock::new((String::new(), Instant::now())),
                stream_token_refresh: tokio::sync::Mutex::new(()),
                context: RwLock::new(Context {
                    languages,
                    ..Context::default()
                }),
                queues: Mutex::new(HashMap::new()),
                opened: Mutex::new(HashMap::new()),
                search: Mutex::new(None),
                record: Mutex::new(None),
            }),
        };

        #[derive(Deserialize)]
        struct MeAnswer {
            me: wire::Me,
        }
        let me: MeAnswer = server.query(queries::ME, json!({})).await?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SettingsAnswer {
            user_settings: wire::UserSettings,
        }
        let settings: SettingsAnswer = server.query(queries::USER_SETTINGS, json!({})).await?;
        let token = server.fetch_stream_token().await?;

        // Nothing else holds the connection yet.
        let inner = Arc::get_mut(&mut server.inner).expect("a new connection is not shared");
        inner.user = User {
            name: me
                .me
                .name
                .filter(|name| !name.is_empty())
                .or_else(|| me.me.email.clone())
                .unwrap_or_default(),
            id: me.me.id,
            email: me.me.email,
        };
        inner
            .context
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .settings = settings.user_settings;
        *inner
            .stream_token
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = (token, Instant::now());
        Ok(server)
    }

    /// Writes every answer of the server into `dir` from now on, named
    /// after the operation; this is how the test data is made.
    #[doc(hidden)]
    pub fn record_into(&self, dir: &std::path::Path) {
        *locked(&self.inner.record) = Some(dir.to_owned());
    }

    pub fn machine_identifier(&self) -> &str {
        &self.inner.machine
    }

    pub fn friendly_name(&self) -> Option<&str> {
        Some(self.inner.well_known.name.as_str()).filter(|name| !name.is_empty())
    }

    /// Where the API of the server is.
    pub fn api_url(&self) -> &str {
        &self.inner.well_known.api_url
    }

    pub fn well_known(&self) -> &WellKnown {
        &self.inner.well_known
    }

    pub fn user(&self) -> &User {
        &self.inner.user
    }

    /// The refresh token as it is now, to keep for the next session.
    pub async fn refresh_token(&self) -> String {
        self.inner.tokens.lock().await.refresh.clone()
    }

    /// Makes the provider forget this session.
    pub async fn sign_out(&self) -> Result<()> {
        let refresh = self.refresh_token().await;
        auth::revoke(&self.inner.http, &self.inner.provider, &refresh).await
    }

    // Requests

    async fn access_token(&self) -> Result<String> {
        let mut tokens = self.inner.tokens.lock().await;
        if !tokens.is_fresh() {
            // One at a time: the provider may accept a refresh token once.
            let renewed =
                auth::refresh(&self.inner.http, &self.inner.provider, &tokens.refresh).await?;
            if renewed.refresh != tokens.refresh {
                (self.inner.keeper)(&renewed.refresh);
            }
            *tokens = renewed;
        }
        Ok(tokens.access.clone())
    }

    async fn request<T: DeserializeOwned>(&self, document: &str, variables: Value) -> Result<T> {
        let url = format!("{}/graphql", self.inner.well_known.api_url);
        let token = self.access_token().await?;
        let response = self
            .inner
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&json!({ "query": document, "variables": variables }))
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::Unauthorized);
        }
        if !status.is_success() {
            return Err(Error::Status { status, url });
        }
        let text = response.text().await?;
        if let Some(dir) = locked(&self.inner.record).as_ref() {
            // Named after the operation, the second word of the document.
            let name = document
                .split(|c: char| !c.is_ascii_alphanumeric())
                .nth(1)
                .unwrap_or_default();
            let _ = std::fs::write(dir.join(format!("{name}.json")), &text);
        }
        let answer: Answer<T> = serde_json::from_str(&text).map_err(|source| Error::Decode {
            url: url.clone(),
            source,
        })?;
        if let Some(error) = answer.errors.first() {
            return Err(match error.message.as_str() {
                "Unauthorized" | "Forbidden" => Error::Unauthorized,
                message => Error::Server(message.to_owned()),
            });
        }
        answer
            .data
            .ok_or_else(|| Error::Server("the server gave no answer".into()))
    }

    /// Sends a document, with a fresh token for media when it is due.
    async fn query<T: DeserializeOwned>(&self, document: &str, variables: Value) -> Result<T> {
        let answer = self.request(document, variables).await?;
        self.renew_stream_token().await;
        Ok(answer)
    }

    async fn fetch_stream_token(&self) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TokenAnswer {
            create_stream_token: wire::StreamToken,
        }
        let answer: TokenAnswer = self
            .request(queries::CREATE_STREAM_TOKEN, json!({}))
            .await?;
        Ok(answer.create_stream_token.token)
    }

    async fn renew_stream_token(&self) {
        let age = |server: &Self| {
            server
                .inner
                .stream_token
                .read()
                .map_or(Duration::ZERO, |token| token.1.elapsed())
        };
        if age(self) < STREAM_TOKEN_AGE {
            return;
        }
        let _one_at_a_time = self.inner.stream_token_refresh.lock().await;
        if age(self) < STREAM_TOKEN_AGE {
            return;
        }
        match self.fetch_stream_token().await {
            Ok(token) => {
                if let Ok(mut current) = self.inner.stream_token.write() {
                    *current = (token, Instant::now());
                }
            }
            Err(err) => eprintln!("Could not renew the token for media: {err}"),
        }
    }

    fn stream_token(&self) -> String {
        self.inner
            .stream_token
            .read()
            .map(|token| token.0.clone())
            .unwrap_or_default()
    }

    fn context(&self) -> Context {
        self.inner
            .context
            .read()
            .map(|context| context.clone())
            .unwrap_or_default()
    }

    // Libraries

    pub async fn sections(&self) -> Result<Vec<LibrarySection>> {
        #[derive(Deserialize)]
        struct Libraries {
            #[serde(default)]
            libraries: Vec<wire::Library>,
        }
        let answer: Libraries = self.query(queries::LIBRARIES, json!({})).await?;
        Ok(answer.libraries.iter().filter_map(map::section).collect())
    }

    /// The newest of every library, a row's worth of each.
    pub async fn recently_added(&self) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        for section in self.sections().await? {
            let newest = match section.kind {
                ItemKind::Artist => self.albums(Some(section.key), None, 0, 20, Some("addedAt:desc")).await?,
                _ => self.section_items(section.key, 0, 20, Some("addedAt:desc")).await?,
            };
            items.extend(newest.items);
        }
        Ok(items)
    }

    pub async fn on_deck(&self) -> Result<Vec<Item>> {
        self.recently_watched(None).await
    }

    pub async fn section_on_deck(&self, section: ItemId) -> Result<Vec<Item>> {
        self.recently_watched(Some(section)).await
    }

    async fn recently_watched(&self, section: Option<ItemId>) -> Result<Vec<Item>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            recently_watched: Option<Vec<wire::Entry>>,
        }
        let library = section.and_then(map::split).map(|(_, uuid)| uuid);
        let answer: Answer = self
            .query(queries::RECENTLY_WATCHED, json!({ "library": library }))
            .await?;
        let context = self.context();
        Ok(answer
            .recently_watched
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| context.entry(entry))
            .collect())
    }

    /// The movies, shows or artists of a library.
    pub async fn section_items(
        &self,
        section: ItemId,
        offset: u32,
        size: u32,
        sort: Option<&str>,
    ) -> Result<Page<Item>> {
        let (kind, library) = map::split(section).ok_or(Error::NotFound(section))?;
        let variables = paging(Some(library), offset, size, sort);
        let context = self.context();
        match kind {
            ItemKind::Movie => {
                #[derive(Deserialize)]
                struct Answer {
                    movies: Option<wire::Page<wire::Movie>>,
                }
                let answer: Answer = self.query(queries::MOVIES, variables).await?;
                Ok(page(answer.movies, offset, |movie| context.movie(movie)))
            }
            ItemKind::Show => {
                #[derive(Deserialize)]
                struct Answer {
                    shows: Option<wire::Page<wire::Show>>,
                }
                let answer: Answer = self.query(queries::SHOWS, variables).await?;
                Ok(page(answer.shows, offset, |show| context.show(show)))
            }
            ItemKind::Artist => {
                #[derive(Deserialize)]
                struct Answer {
                    persons: Option<wire::Page<wire::Person>>,
                }
                let answer: Answer = self.query(queries::ARTISTS, variables).await?;
                Ok(page(answer.persons, offset, |person| context.artist(person)))
            }
            _ => Err(Error::NotFound(section)),
        }
    }

    async fn albums(
        &self,
        section: Option<ItemId>,
        artist: Option<&str>,
        offset: u32,
        size: u32,
        sort: Option<&str>,
    ) -> Result<Page<Item>> {
        #[derive(Deserialize)]
        struct Answer {
            albums: Option<wire::Page<wire::Album>>,
        }
        let library = section.and_then(map::split).map(|(_, uuid)| uuid);
        let mut variables = paging(library, offset, size, sort);
        variables["artist"] = json!(artist);
        let answer: Answer = self.query(queries::ALBUMS, variables).await?;
        let context = self.context();
        Ok(page(answer.albums, offset, |album| context.album(album)))
    }

    /// Every album of a music library.
    pub async fn section_albums(&self, section: ItemId) -> Result<Vec<Item>> {
        let mut albums = Vec::new();
        loop {
            let more = self
                .albums(Some(section), None, albums.len() as u32, 200, None)
                .await?;
            let done = more.items.is_empty();
            albums.extend(more.items);
            if done || albums.len() as u32 >= more.total {
                return Ok(albums);
            }
        }
    }

    /// The orders a library can be shown in, named as Plex names them so
    /// that the interface knows what to call them.
    pub async fn section_sorts(&self, _section: ItemId) -> Result<Vec<SortOption>> {
        let option = |key: &str, title: &str, direction: &str| SortOption {
            key: key.to_owned(),
            title: title.to_owned(),
            default_direction: Some(direction.to_owned()),
        };
        Ok(vec![
            option("titleSort", "Title", "asc"),
            option("originallyAvailableAt", "Release year", "desc"),
            option("addedAt", "Date added", "desc"),
        ])
    }

    // Items

    pub async fn item(&self, id: ItemId) -> Result<Item> {
        let (kind, uuid) = map::split(id).ok_or(Error::NotFound(id))?;
        let variables = json!({ "id": uuid });
        let context = self.context();
        let missing = || Error::NotFound(id);
        match kind {
            ItemKind::Movie => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Answer {
                    movie_by_id: Option<wire::Movie>,
                }
                let answer: Answer = self.query(queries::MOVIE, variables).await?;
                Ok(context.movie(&answer.movie_by_id.ok_or_else(missing)?))
            }
            ItemKind::Show => Ok(context.show(&self.show(id).await?)),
            ItemKind::Season => Ok(context.season(&self.season(id).await?)),
            ItemKind::Episode => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Answer {
                    episode_by_id: Option<wire::Episode>,
                }
                let answer: Answer = self.query(queries::EPISODE, variables).await?;
                Ok(context.episode(&answer.episode_by_id.ok_or_else(missing)?))
            }
            ItemKind::Artist => Ok(context.artist(&self.artist(id).await?)),
            ItemKind::Album => Ok(context.album(&self.album(id).await?)),
            ItemKind::Track => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Answer {
                    track_by_id: Option<wire::Track>,
                }
                let answer: Answer = self.query(queries::TRACK, variables).await?;
                Ok(context.track(&answer.track_by_id.ok_or_else(missing)?))
            }
            ItemKind::Playlist => Ok(context.playlist(&self.playlist(id).await?)),
            _ => Err(missing()),
        }
    }

    async fn show(&self, id: ItemId) -> Result<wire::Show> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            show_by_id: Option<wire::Show>,
        }
        let answer: Answer = self.query(queries::SHOW, by_id(id)?).await?;
        answer.show_by_id.ok_or(Error::NotFound(id))
    }

    async fn season(&self, id: ItemId) -> Result<wire::Season> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            season_by_id: Option<wire::Season>,
        }
        let answer: Answer = self.query(queries::SEASON, by_id(id)?).await?;
        answer.season_by_id.ok_or(Error::NotFound(id))
    }

    async fn artist(&self, id: ItemId) -> Result<wire::Person> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            person_by_id: Option<wire::Person>,
        }
        let answer: Answer = self.query(queries::ARTIST, by_id(id)?).await?;
        answer.person_by_id.ok_or(Error::NotFound(id))
    }

    async fn album(&self, id: ItemId) -> Result<wire::Album> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            album_by_id: Option<wire::Album>,
        }
        let answer: Answer = self.query(queries::ALBUM, by_id(id)?).await?;
        answer.album_by_id.ok_or(Error::NotFound(id))
    }

    async fn playlist(&self, id: ItemId) -> Result<wire::Playlist> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            playlist_by_id: Option<wire::Playlist>,
        }
        let answer: Answer = self.query(queries::PLAYLIST, by_id(id)?).await?;
        answer.playlist_by_id.ok_or(Error::NotFound(id))
    }

    /// The seasons of a show, the episodes of a season, the albums of an
    /// artist or the tracks of an album.
    pub async fn children(&self, id: ItemId) -> Result<Vec<Item>> {
        let (kind, _) = map::split(id).ok_or(Error::NotFound(id))?;
        let context = self.context();
        match kind {
            ItemKind::Show => Ok(context.seasons(&self.show(id).await?)),
            ItemKind::Season => {
                let season = self.season(id).await?;
                let mut episodes: Vec<Item> = season
                    .episodes
                    .iter()
                    .map(|episode| context.episode(episode))
                    .collect();
                episodes.sort_by_key(|episode| episode.index);
                Ok(episodes)
            }
            ItemKind::Artist => {
                let artist = self.artist(id).await?;
                Ok(artist
                    .albums
                    .iter()
                    .flatten()
                    .map(|album| context.album(album))
                    .collect())
            }
            ItemKind::Album => {
                let album = self.album(id).await?;
                let mut tracks: Vec<Item> = album
                    .tracks
                    .iter()
                    .flatten()
                    .map(|track| context.track(track))
                    .collect();
                tracks.sort_by_key(|track| (track.parent_index, track.index));
                Ok(tracks)
            }
            _ => Err(Error::NotFound(id)),
        }
    }

    pub async fn children_page(&self, id: ItemId, offset: u32, size: u32) -> Result<Page<Item>> {
        Ok(slice(self.children(id).await?, offset, size))
    }

    /// Every episode of a show or every track of an artist.
    pub async fn all_leaves(&self, id: ItemId) -> Result<Vec<Item>> {
        let (kind, uuid) = map::split(id).ok_or(Error::NotFound(id))?;
        let context = self.context();
        match kind {
            ItemKind::Show => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Answer {
                    show_by_id: Option<wire::ShowEpisodes>,
                }
                let answer: Answer = self.query(queries::SHOW_EPISODES, by_id(id)?).await?;
                let show = answer.show_by_id.ok_or(Error::NotFound(id))?;
                let mut episodes: Vec<Item> = show
                    .episodes
                    .iter()
                    .map(|episode| context.episode(episode))
                    .collect();
                episodes.sort_by_key(|episode| (episode.parent_index, episode.index));
                Ok(episodes)
            }
            ItemKind::Artist => {
                #[derive(Deserialize)]
                struct Answer {
                    tracks: Option<wire::Page<wire::Track>>,
                }
                let mut tracks = Vec::new();
                loop {
                    let variables =
                        json!({ "artist": uuid, "page": tracks.len() / 200, "size": 200 });
                    let answer: Answer = self.query(queries::TRACKS, variables).await?;
                    let more = page(answer.tracks, tracks.len() as u32, |track| {
                        context.track(track)
                    });
                    let done = more.items.is_empty();
                    tracks.extend(more.items);
                    if done || tracks.len() as u32 >= more.total {
                        return Ok(tracks);
                    }
                }
            }
            ItemKind::Season | ItemKind::Album => self.children(id).await,
            _ => Err(Error::NotFound(id)),
        }
    }

    pub async fn playlists(&self) -> Result<Vec<Item>> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            playlists: Vec<wire::Playlist>,
        }
        let answer: Answer = self.query(queries::PLAYLISTS, json!({})).await?;
        let context = self.context();
        Ok(answer
            .playlists
            .iter()
            // Playlists of books and podcasts hold nothing Girens plays.
            .filter(|playlist| matches!(playlist.library_type.as_str(), "MOVIE" | "SHOW" | "MUSIC"))
            .map(|playlist| context.playlist(playlist))
            .collect())
    }

    pub async fn playlist_items(&self, id: ItemId) -> Result<Vec<Item>> {
        let playlist = self.playlist(id).await?;
        let context = self.context();
        Ok(playlist
            .items
            .iter()
            .flatten()
            .filter_map(|entry| context.entry(entry))
            .collect())
    }

    pub async fn playlist_page(&self, id: ItemId, offset: u32, size: u32) -> Result<Page<Item>> {
        Ok(slice(self.playlist_items(id).await?, offset, size))
    }

    /// The server searches everything at once; the results are kept for a
    /// moment, for when the next kind is asked for.
    pub async fn search(&self, query: &str, kind: ItemKind, limit: u32) -> Result<Vec<Item>> {
        let kept = locked(&self.inner.search)
            .as_ref()
            .filter(|(kept, at, _)| kept == query && at.elapsed() < SEARCH_AGE)
            .map(|(_, _, items)| items.clone());
        let items = match kept {
            Some(items) => items,
            None => {
                #[derive(Deserialize)]
                struct Answer {
                    #[serde(default)]
                    search: Vec<wire::SearchResult>,
                }
                let variables = json!({ "term": query, "size": 50 });
                let answer: Answer = self.query(queries::SEARCH, variables).await?;
                let context = self.context();
                let items: Vec<Item> = answer
                    .search
                    .iter()
                    .filter_map(|result| context.search_result(result))
                    .collect();
                *locked(&self.inner.search) = Some((query.to_owned(), Instant::now(), items.clone()));
                items
            }
        };
        Ok(items
            .into_iter()
            .filter(|item| item.kind == kind)
            .take(limit as usize)
            .collect())
    }

    // What the user did with an item

    pub async fn mark_watched(&self, id: ItemId) -> Result<()> {
        for (media_type, uuid) in self.playable(id).await? {
            let variables = json!({ "type": media_type, "id": uuid });
            let _: Value = self.query(queries::MARK_PLAYED, variables).await?;
        }
        Ok(())
    }

    /// Forgets every play of the item: the server keeps a history rather
    /// than a mark.
    pub async fn mark_unwatched(&self, id: ItemId) -> Result<()> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct History {
            #[serde(default)]
            playback_history: Vec<wire::Id>,
        }
        for (media_type, uuid) in self.playable(id).await? {
            let variables = json!({ "type": media_type, "id": uuid });
            let history: History = self.query(queries::PLAYBACK_HISTORY, variables).await?;
            for entry in history.playback_history {
                let _: Value = self
                    .query(queries::DELETE_WATCH_STATUS, json!({ "id": entry.id }))
                    .await?;
            }
        }
        Ok(())
    }

    /// The movies, episodes and tracks that `id` stands for, as the server
    /// names their type.
    async fn playable(&self, id: ItemId) -> Result<Vec<(&'static str, String)>> {
        let (kind, uuid) = map::split(id).ok_or(Error::NotFound(id))?;
        let uuid_of = |item: &Item| {
            map::split(item.rating_key)
                .map(|(_, uuid)| uuid.to_owned())
                .unwrap_or_default()
        };
        Ok(match kind {
            ItemKind::Movie => vec![("MOVIE", uuid.to_owned())],
            ItemKind::Episode => vec![("EPISODE", uuid.to_owned())],
            ItemKind::Track => vec![("TRACK", uuid.to_owned())],
            ItemKind::Show | ItemKind::Season => self
                .all_leaves(id)
                .await?
                .iter()
                .map(|episode| ("EPISODE", uuid_of(episode)))
                .collect(),
            ItemKind::Album => self
                .children(id)
                .await?
                .iter()
                .map(|track| ("TRACK", uuid_of(track)))
                .collect(),
            _ => return Err(Error::NotFound(id)),
        })
    }

    /// `rating` runs from 0 to 10; 0 takes the rating away.
    pub async fn rate(&self, id: ItemId, rating: f32) -> Result<()> {
        let (kind, uuid) = map::split(id).ok_or(Error::NotFound(id))?;
        let media_type = match kind {
            ItemKind::Movie => "MOVIE",
            ItemKind::Show => "SHOW",
            ItemKind::Episode => "EPISODE",
            ItemKind::Album => "ALBUM",
            ItemKind::Track => "TRACK",
            _ => return Err(Error::NotFound(id)),
        };
        let rating = rating.round().clamp(0.0, 10.0) as u32;
        let rating = (rating > 0).then_some(rating);
        let variables = json!({ "type": media_type, "id": uuid, "rating": rating });
        let _: Value = self.query(queries::SET_RATING, variables).await?;
        Ok(())
    }

    /// Remembers the choice for as long as this program runs; the server
    /// has every stream ready and does not need to hear.
    pub async fn select_audio_stream(&self, part: &Part, stream: ItemId) -> Result<()> {
        self.select(part, |selection| selection.audio = Some(stream));
        Ok(())
    }

    /// `None` turns subtitles off.
    pub async fn select_subtitle_stream(&self, part: &Part, stream: Option<ItemId>) -> Result<()> {
        self.select(part, |selection| selection.subtitle = Some(stream));
        Ok(())
    }

    fn select(&self, part: &Part, change: impl FnOnce(&mut Selection)) {
        if let Ok(mut context) = self.inner.context.write() {
            change(context.selections.entry(part.id).or_default());
        }
    }

    // Addresses

    /// Where to get the image `thumb` of an item, about `width` wide.
    pub fn image_url(&self, thumb: &str, width: u32, _height: u32) -> String {
        let token = self.stream_token();
        match IMAGE_WIDTHS.iter().find(|bucket| **bucket >= width) {
            Some(bucket) => format!("{thumb}?width={bucket}&token={token}"),
            // Wider than the server keeps: the original.
            None => format!("{thumb}?token={token}"),
        }
    }

    /// Where to get a file of an item, such as subtitles (the `key` of the
    /// stream).
    pub fn download_url(&self, key: &str) -> String {
        format!("{key}?token={}", self.stream_token())
    }

    // Playing

    /// What mpv plays for `item`: the server's stream of the file as it is
    /// (`direct`) or converted.
    pub fn stream_url(&self, item: &Item, direct: bool) -> Result<String> {
        let part = item
            .parts()
            .next()
            .ok_or(Error::NotFound(item.rating_key))?;
        locked(&self.inner.opened).insert(
            item.rating_key,
            Opened {
                direct,
                file: part.id,
            },
        );
        Ok(format!(
            "{}/master.m3u8?direct={direct}&transcode={}&subtitleFormat=SRT&token={}",
            part.key,
            !direct,
            self.stream_token()
        ))
    }

    /// Reports where playback of `item`, an entry of a play queue, is. This
    /// is also what tells the server that the player is still there.
    pub async fn update_timeline(
        &self,
        item: &Item,
        position: u64,
        state: PlaybackState,
        _session: Option<&str>,
    ) -> Result<()> {
        // Without a queue on the server there is nothing to report to.
        let Some((queue, entry)) = item
            .play_queue_item_id
            .and_then(|id| id.as_str().split_once(':'))
        else {
            return Ok(());
        };
        let opened = locked(&self.inner.opened).get(&item.rating_key).copied();
        let settings = opened.map(|opened| {
            json!({
                "direct": opened.direct,
                "transcode": !opened.direct,
                "subtitleFormat": "SRT",
                "mediaFileId": opened.file.as_str(),
            })
        });
        let state = match state {
            PlaybackState::Playing => "PLAYING",
            // The server has no word for stopped: it notices by itself
            // that the reports end.
            PlaybackState::Paused | PlaybackState::Stopped => "PAUSED",
        };
        let variables = json!({
            "id": queue,
            "item": entry,
            "progress": position.min(i32::MAX as u64),
            "settings": settings,
            "state": state,
            "device": self.inner.device,
        });
        let _: Value = self.query(queries::UPDATE_PLAY_QUEUE, variables).await?;
        Ok(())
    }

    // Play queues

    /// Queue for `item`. A show continues with the next episodes.
    pub async fn create_play_queue(
        &self,
        item: &Item,
        start_item: Option<&Item>,
        shuffle: bool,
    ) -> Result<PlayQueue> {
        let uuid = |id: ItemId| map::split(id).map(|(_, uuid)| uuid);
        let source = uuid(item.rating_key).ok_or(Error::NotFound(item.rating_key))?;
        let start = start_item.and_then(|start| uuid(start.rating_key));
        let mut input = match item.kind {
            ItemKind::Movie => json!({ "sourceType": "MOVIE", "sourceId": source }),
            ItemKind::Show => json!({ "sourceType": "SHOW", "sourceId": source, "startId": start }),
            // A season or an episode plays within its show.
            ItemKind::Season => {
                let show = item.parent_rating_key.and_then(uuid);
                let first = match start {
                    Some(start) => Some(start.to_owned()),
                    None => self
                        .children(item.rating_key)
                        .await?
                        .first()
                        .and_then(|episode| uuid(episode.rating_key))
                        .map(str::to_owned),
                };
                json!({ "sourceType": "SHOW", "sourceId": show, "startId": first })
            }
            ItemKind::Episode => {
                let show = item.grandparent_rating_key.and_then(uuid);
                json!({ "sourceType": "SHOW", "sourceId": show, "startId": source })
            }
            ItemKind::Album => json!({ "sourceType": "ALBUM", "sourceId": source, "startId": start }),
            ItemKind::Track => {
                let album = item.parent_rating_key.and_then(uuid);
                json!({ "sourceType": "ALBUM", "sourceId": album, "startId": source })
            }
            ItemKind::Artist if shuffle => json!({ "sourceType": "ARTIST", "sourceId": source }),
            ItemKind::Artist => {
                json!({ "sourceType": "ARTIST", "sourceId": source, "rankKind": "MOST_PLAYED" })
            }
            ItemKind::Playlist => {
                json!({ "sourceType": "PLAYLIST", "sourceId": source, "startId": start })
            }
            _ => return Err(Error::NotFound(item.rating_key)),
        };
        input["shuffle"] = json!(shuffle);
        self.new_play_queue(input).await
    }

    /// Queue with everything in a library, which the server only plays in
    /// a random order.
    pub async fn create_section_play_queue(
        &self,
        section: &LibrarySection,
        _sort: Option<&str>,
        _shuffle: bool,
    ) -> Result<PlayQueue> {
        let input = json!({ "sourceType": "LIBRARY", "sourceId": section.uuid, "shuffle": true });
        self.new_play_queue(input).await
    }

    async fn new_play_queue(&self, input: Value) -> Result<PlayQueue> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            create_play_queue: wire::PlayQueue,
        }
        let variables = json!({ "input": input });
        let answer: Answer = self.query(queries::CREATE_PLAY_QUEUE, variables).await?;
        let queue = self.play_queue(&answer.create_play_queue);
        locked(&self.inner.queues).insert(queue.id, QueueSource { input });
        Ok(queue)
    }

    fn play_queue(&self, queue: &wire::PlayQueue) -> PlayQueue {
        let context = self.context();
        // The entry of a queue is told to the server together with the
        // queue, so both go in its id.
        let entry_id = |entry: &str| ItemId::new(&format!("{}:{entry}", queue.id));
        let items = queue
            .play_queue_items
            .iter()
            .filter_map(|entry| {
                let mut item = context.entry(entry)?;
                item.play_queue_item_id = entry.id.as_deref().map(entry_id);
                Some(item)
            })
            .collect();
        PlayQueue {
            id: ItemId::new(&queue.id),
            selected_item_id: queue.current_item_id.as_deref().map(entry_id),
            shuffled: queue.shuffle,
            items,
        }
    }

    /// Fetches the current state of a queue, which grows while it plays.
    pub async fn refresh_play_queue(&self, id: ItemId) -> Result<PlayQueue> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer {
            get_play_queue: Option<wire::PlayQueue>,
        }
        let variables = json!({ "id": id.as_str() });
        let answer: Answer = self.query(queries::GET_PLAY_QUEUE, variables).await?;
        Ok(self.play_queue(&answer.get_play_queue.ok_or(Error::NotFound(id))?))
    }

    /// The server cannot reorder a queue, so this makes a new one of the
    /// same source, starting at what is playing.
    pub async fn shuffle_play_queue(&self, id: ItemId, shuffle: bool) -> Result<PlayQueue> {
        let source = locked(&self.inner.queues).get(&id).cloned();
        let Some(source) = source else {
            return Err(Error::NotFound(id));
        };
        let current = self.refresh_play_queue(id).await?;
        let playing = current
            .selected_index()
            .and_then(|index| current.items.get(index))
            .and_then(|item| map::split(item.rating_key))
            .map(|(_, uuid)| uuid);
        let mut input = source.input;
        input["shuffle"] = json!(shuffle);
        // A movie is its own queue, and a library only plays shuffled.
        match input["sourceType"].as_str() {
            Some("MOVIE") => {}
            Some("LIBRARY") => input["shuffle"] = json!(true),
            _ => input["startId"] = json!(playing),
        }
        self.new_play_queue(input).await
    }
}

fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(index, c)| match index {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn by_id(id: ItemId) -> Result<Value> {
    let (_, uuid) = map::split(id).ok_or(Error::NotFound(id))?;
    Ok(json!({ "id": uuid }))
}

/// The variables of a paged, sorted list. `sort` is `<key>:<asc|desc>` with
/// the keys of [`IsterServer::section_sorts`].
fn paging(library: Option<&str>, offset: u32, size: u32, sort: Option<&str>) -> Value {
    let size = size.clamp(1, 200);
    let (key, direction) = sort
        .and_then(|sort| sort.split_once(':'))
        .unwrap_or(("titleSort", "asc"));
    let sorting = match key {
        "addedAt" => "DATE_CREATED",
        "originallyAvailableAt" | "year" => "RELEASE_YEAR",
        _ => "NAME",
    };
    let order = if direction == "desc" {
        "DESCENDING"
    } else {
        "ASCENDING"
    };
    json!({
        "library": library,
        "page": offset / size,
        "size": size,
        "sorting": sorting,
        "order": order,
    })
}

fn page<T>(answer: Option<wire::Page<T>>, offset: u32, item: impl Fn(&T) -> Item) -> Page<Item> {
    match answer {
        Some(answer) => Page {
            items: answer.content.iter().map(item).collect(),
            offset,
            total: answer.total_elements,
        },
        None => Page {
            items: Vec::new(),
            offset,
            total: 0,
        },
    }
}

/// A page of a list the server only gives whole.
fn slice(items: Vec<Item>, offset: u32, size: u32) -> Page<Item> {
    let total = items.len() as u32;
    Page {
        items: items
            .into_iter()
            .skip(offset as usize)
            .take(size as usize)
            .collect(),
        offset,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_names_the_server() {
        assert_eq!(
            machine_identifier("https://Media.Example.org/api/"),
            "ister_media.example.org_api"
        );
        assert_eq!(machine_identifier("http://localhost:8080"), "ister_localhost_8080");
    }

    #[test]
    fn sorting_is_told_as_the_server_names_it() {
        let variables = paging(Some("lib"), 100, 50, Some("addedAt:desc"));
        assert_eq!(variables["page"], 2);
        assert_eq!(variables["sorting"], "DATE_CREATED");
        assert_eq!(variables["order"], "DESCENDING");
        let variables = paging(None, 0, 500, None);
        assert_eq!(variables["size"], 200);
        assert_eq!(variables["sorting"], "NAME");
        assert_eq!(variables["order"], "ASCENDING");
    }
}
