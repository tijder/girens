use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use url::Url;

use crate::model::{Item, ItemKind, LibrarySection, Part, SortOption};
use crate::Page;
use girens_model::ItemId;
use crate::{Error, Result};

pub(crate) const LIBRARY_IDENTIFIER: &str = "com.plexapp.plugins.library";

/// How this application introduces itself to Plex.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Stable per installation; Plex lists devices by it.
    pub identifier: String,
    pub product: String,
    pub version: String,
    pub device_name: String,
}

impl ClientInfo {
    pub(crate) fn headers(&self, token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let mut set = |name: &'static str, value: &str| {
            if let Ok(value) = HeaderValue::from_str(value) {
                headers.insert(HeaderName::from_static(name), value);
            }
        };
        set("x-plex-client-identifier", &self.identifier);
        set("x-plex-product", &self.product);
        set("x-plex-version", &self.version);
        set("x-plex-device-name", &self.device_name);
        set("x-plex-platform", "Linux");
        if let Some(token) = token {
            set("x-plex-token", token);
        }
        headers
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerIdentity {
    machine_identifier: String,
    friendly_name: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Envelope<T> {
    #[serde(rename = "MediaContainer")]
    pub container: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Items {
    #[serde(default, rename = "Metadata")]
    metadata: Vec<Item>,
    offset: Option<u32>,
    total_size: Option<u32>,
}

#[derive(Deserialize)]
struct Directories<T> {
    #[serde(default = "Vec::new", rename = "Directory")]
    directory: Vec<T>,
}

pub(crate) async fn send<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T> {
    let response = request.send().await?;
    let status = response.status();
    let url = redact(response.url());
    if status == StatusCode::UNAUTHORIZED {
        return Err(Error::Unauthorized);
    }
    if !status.is_success() {
        return Err(Error::Status { status, url });
    }
    let body = response.bytes().await?;
    // Commands such as scrobble answer with an empty body.
    let body: &[u8] = if body.is_empty() { b"null" } else { &body };
    serde_json::from_slice(body).map_err(|source| Error::Decode { url, source })
}

/// URLs in errors end up in logs; never include the token there.
fn redact(url: &Url) -> String {
    let mut url = url.clone();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(name, _)| name != "X-Plex-Token")
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

/// A connection to one Plex Media Server.
#[derive(Debug, Clone)]
pub struct PlexServer {
    http: reqwest::Client,
    base: Url,
    token: String,
    machine_identifier: String,
    friendly_name: Option<String>,
}

impl PlexServer {
    /// Connects to `base_url` and verifies the token.
    pub async fn connect(base_url: &str, token: &str, client: &ClientInfo) -> Result<Self> {
        let http = reqwest::Client::builder()
            .default_headers(client.headers(Some(token)))
            .build()?;
        Self::connect_with(http, base_url, token).await
    }

    pub(crate) async fn connect_with(
        http: reqwest::Client,
        base_url: &str,
        token: &str,
    ) -> Result<Self> {
        let mut base = Url::parse(base_url)?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        let mut server = Self {
            http,
            base,
            token: token.to_owned(),
            machine_identifier: String::new(),
            friendly_name: None,
        };
        // The root is only served to authenticated clients, unlike /identity.
        let root: Envelope<ServerIdentity> = server.get("", &[]).await?;
        server.machine_identifier = root.container.machine_identifier;
        server.friendly_name = root.container.friendly_name;
        Ok(server)
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn machine_identifier(&self) -> &str {
        &self.machine_identifier
    }

    pub fn friendly_name(&self) -> Option<&str> {
        self.friendly_name.as_deref()
    }

    /// Absolute URL for a server path such as `/library/parts/1/2/file.mkv`.
    /// `path` may already contain a query string.
    pub fn url(&self, path: &str, params: &[(&str, &str)], include_token: bool) -> Url {
        let (path, query) = match path.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (path, None),
        };
        let mut url = self.base.clone();
        let joined = format!(
            "{}{}",
            url.path().trim_end_matches('/'),
            if path.is_empty() || path.starts_with('/') {
                path.to_owned()
            } else {
                format!("/{path}")
            }
        );
        url.set_path(if joined.is_empty() { "/" } else { &joined });
        url.set_query(query);
        if !params.is_empty() || include_token {
            let mut pairs = url.query_pairs_mut();
            pairs.extend_pairs(params);
            if include_token {
                pairs.append_pair("X-Plex-Token", &self.token);
            }
        }
        url
    }

    pub(crate) async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        send(self.http.request(method, self.url(path, params, false))).await
    }

    /// Like [`Self::request`], but introduces the client as `platform`
    /// instead of the default from [`ClientInfo`].
    pub(crate) async fn request_as_platform<T: DeserializeOwned>(
        &self,
        platform: &'static str,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        send(
            self.http
                .get(self.url(path, params, false))
                .header("x-plex-platform", platform),
        )
        .await
    }

    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        self.request(Method::GET, path, params).await
    }

    async fn command(&self, method: Method, path: &str, params: &[(&str, &str)]) -> Result<()> {
        self.request::<serde::de::IgnoredAny>(method, path, params)
            .await
            .map(drop)
    }

    async fn items(&self, path: &str, params: &[(&str, &str)]) -> Result<Vec<Item>> {
        let envelope: Envelope<Items> = self.get(path, params).await?;
        Ok(envelope.container.metadata)
    }

    // Library browsing

    pub async fn sections(&self) -> Result<Vec<LibrarySection>> {
        let envelope: Envelope<Directories<LibrarySection>> =
            self.get("/library/sections", &[]).await?;
        Ok(envelope.container.directory)
    }

    pub async fn recently_added(&self) -> Result<Vec<Item>> {
        self.items("/library/recentlyAdded", &[]).await
    }

    pub async fn on_deck(&self) -> Result<Vec<Item>> {
        self.items("/library/onDeck", &[]).await
    }

    pub async fn section_on_deck(&self, section: ItemId) -> Result<Vec<Item>> {
        self.items(&format!("/library/sections/{section}/onDeck"), &[])
            .await
    }

    /// One page of a section, optionally sorted (`titleSort:asc`).
    pub async fn section_items(
        &self,
        section: ItemId,
        offset: u32,
        size: u32,
        sort: Option<&str>,
    ) -> Result<Page<Item>> {
        self.page(&format!("/library/sections/{section}/all"), offset, size, sort)
            .await
    }

    /// `size` items of the list at `path`, from `offset`.
    async fn page(
        &self,
        path: &str,
        offset: u32,
        size: u32,
        sort: Option<&str>,
    ) -> Result<Page<Item>> {
        let (offset_text, size_text) = (offset.to_string(), size.to_string());
        let mut params = vec![
            ("X-Plex-Container-Start", offset_text.as_str()),
            ("X-Plex-Container-Size", size_text.as_str()),
        ];
        if let Some(sort) = sort {
            params.push(("sort", sort));
        }
        let envelope: Envelope<Items> = self.get(path, &params).await?;
        let container = envelope.container;
        Ok(Page {
            offset: container.offset.unwrap_or(offset),
            total: container
                .total_size
                .unwrap_or(offset + container.metadata.len() as u32),
            items: container.metadata,
        })
    }

    pub async fn section_albums(&self, section: ItemId) -> Result<Vec<Item>> {
        self.items(&format!("/library/sections/{section}/albums"), &[])
            .await
    }

    pub async fn section_sorts(&self, section: ItemId) -> Result<Vec<SortOption>> {
        let envelope: Envelope<Directories<SortOption>> = self
            .get(&format!("/library/sections/{section}/sorts"), &[])
            .await?;
        Ok(envelope.container.directory)
    }

    pub async fn item(&self, rating_key: ItemId) -> Result<Item> {
        self.items(&format!("/library/metadata/{rating_key}"), &[])
            .await?
            .into_iter()
            .next()
            .ok_or(Error::NotFound(rating_key))
    }

    /// Seasons of a show, albums of an artist or tracks of an album.
    pub async fn children(&self, rating_key: ItemId) -> Result<Vec<Item>> {
        self.items(&format!("/library/metadata/{rating_key}/children"), &[])
            .await
    }

    /// One page of what is in a photo album, which may hold thousands of
    /// photos.
    pub async fn children_page(&self, rating_key: ItemId, offset: u32, size: u32) -> Result<Page<Item>> {
        let path = format!("/library/metadata/{rating_key}/children");
        self.page(&path, offset, size, None).await
    }

    /// All episodes of a show or all tracks of an artist.
    pub async fn all_leaves(&self, rating_key: ItemId) -> Result<Vec<Item>> {
        self.items(&format!("/library/metadata/{rating_key}/allLeaves"), &[])
            .await
    }

    pub async fn playlists(&self) -> Result<Vec<Item>> {
        self.items("/playlists", &[]).await
    }

    pub async fn playlist_items(&self, rating_key: ItemId) -> Result<Vec<Item>> {
        self.items(&format!("/playlists/{rating_key}/items"), &[])
            .await
    }

    /// One page of a playlist, which may hold thousands of items.
    pub async fn playlist_page(&self, rating_key: ItemId, offset: u32, size: u32) -> Result<Page<Item>> {
        self.page(&format!("/playlists/{rating_key}/items"), offset, size, None)
            .await
    }

    pub async fn search(&self, query: &str, kind: ItemKind, limit: u32) -> Result<Vec<Item>> {
        let limit = limit.to_string();
        let kind = kind.search_type().map(|kind| kind.to_string());
        let mut params = vec![("title", query), ("X-Plex-Container-Size", limit.as_str())];
        params.push(("X-Plex-Container-Start", "0"));
        if let Some(kind) = &kind {
            params.push(("type", kind));
        }
        self.items("/library/all", &params).await
    }

    // Watched state and rating

    pub async fn mark_watched(&self, rating_key: ItemId) -> Result<()> {
        self.scrobble("/:/scrobble", rating_key).await
    }

    pub async fn mark_unwatched(&self, rating_key: ItemId) -> Result<()> {
        self.scrobble("/:/unscrobble", rating_key).await
    }

    async fn scrobble(&self, path: &str, rating_key: ItemId) -> Result<()> {
        let key = rating_key.to_string();
        self.command(
            Method::GET,
            path,
            &[("key", &key), ("identifier", LIBRARY_IDENTIFIER)],
        )
        .await
    }

    /// `rating` runs from 0 to 10.
    pub async fn rate(&self, rating_key: ItemId, rating: f32) -> Result<()> {
        let (key, rating) = (rating_key.to_string(), rating.to_string());
        self.command(
            Method::PUT,
            "/:/rate",
            &[
                ("key", &key),
                ("identifier", LIBRARY_IDENTIFIER),
                ("rating", &rating),
            ],
        )
        .await
    }

    // Stream selection (stored on the server per user)

    pub async fn select_audio_stream(&self, part: &Part, stream_id: ItemId) -> Result<()> {
        self.select_stream(part, "audioStreamID", stream_id).await
    }

    /// `None` turns subtitles off.
    pub async fn select_subtitle_stream(&self, part: &Part, stream_id: Option<ItemId>) -> Result<()> {
        self.select_stream(part, "subtitleStreamID", stream_id.unwrap_or(ItemId::fixed("0")))
            .await
    }

    async fn select_stream(&self, part: &Part, name: &str, stream_id: ItemId) -> Result<()> {
        let stream_id = stream_id.to_string();
        self.command(
            Method::PUT,
            &format!("/library/parts/{}", part.id),
            &[("allParts", "1"), (name, &stream_id)],
        )
        .await
    }

    // URLs handed to mpv or to the image loader

    /// A server-side scaled version of `thumb` (an item's `thumb`, `art`, …).
    pub fn image_url(&self, thumb: &str, width: u32, height: u32) -> Url {
        let (width, height) = (width.to_string(), height.to_string());
        self.url(
            "/photo/:/transcode",
            &[
                ("url", thumb),
                ("width", &width),
                ("height", &height),
                ("minSize", "1"),
                ("upscale", "1"),
            ],
            true,
        )
    }

    /// The original media file, for direct play.
    pub fn part_url(&self, part: &Part) -> Url {
        self.url(&part.key, &[], true)
    }

    /// The original media file as a download (also used for sidecar subtitles).
    pub fn download_url(&self, key: &str) -> Url {
        self.url(key, &[("download", "1")], true)
    }
}
