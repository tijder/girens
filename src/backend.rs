//! The media server in use, whichever kind it is.
//!
//! Every kind offers the same operations under the same names; what one
//! cannot do is told by [`Server::supports`], so that the interface can
//! leave it out.

use girens_ister::IsterServer;
use girens_model::model::{Item, ItemKind, LibrarySection, Part, SortOption};
use girens_model::subtitles::PictureSubtitles;
use girens_model::{ItemId, Page, PlayQueue, PlaybackState, Result};
use girens_plex::playback::TranscodeRequest;
use girens_plex::PlexServer;

/// Something not every kind of server can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Keeping copies for watching offline.
    Downloads,
    /// Choosing the resolution to convert a video to.
    TranscodeResolution,
}

/// How to play an item.
#[derive(Debug, Clone)]
pub struct StreamOptions {
    /// Play the file as it is, rather than have the server convert it.
    pub direct: bool,
    /// What to convert to, as `<width>x<height>`.
    pub resolution: Option<String>,
}

/// A stream the server opened for an item.
#[derive(Debug, Clone)]
pub struct Opened {
    /// What mpv plays.
    pub url: String,
    /// Set when the server converts the item: the session to keep alive
    /// while playing and to end afterwards.
    pub session: Option<String>,
    /// Seconds to shift the subtitles that are loaded beside the stream.
    pub subtitle_delay: f64,
    /// Whether the stream comes as a list of many short files.
    pub segmented: bool,
}

/// Where a copy of an item comes from.
#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    /// Set when the server converts the item: the session to end afterwards.
    pub session: Option<String>,
}

#[derive(Clone)]
pub enum Server {
    Plex(PlexServer),
    Ister(IsterServer),
}

/// Calls the same method on whichever server this is.
macro_rules! each {
    ($self:ident, $server:ident => $call:expr) => {
        match $self {
            Server::Plex($server) => $call,
            Server::Ister($server) => $call,
        }
    };
}

impl From<PlexServer> for Server {
    fn from(server: PlexServer) -> Self {
        Server::Plex(server)
    }
}

impl From<IsterServer> for Server {
    fn from(server: IsterServer) -> Self {
        Server::Ister(server)
    }
}

impl Server {
    pub fn as_ister(&self) -> Option<&IsterServer> {
        match self {
            Server::Ister(server) => Some(server),
            Server::Plex(_) => None,
        }
    }

    pub fn supports(&self, feature: Feature) -> bool {
        match (self, feature) {
            (Server::Plex(_), _) => true,
            // Ister only hands out streams, and decides their sizes itself.
            (Server::Ister(_), Feature::Downloads | Feature::TranscodeResolution) => false,
        }
    }

    /// Whether the tracks mpv finds in a stream are those of the file in the
    /// same order, rather than numbered as in the file.
    pub fn streams_in_order(&self) -> bool {
        matches!(self, Server::Ister(_))
    }

    /// Whether the server forgets a player that does not report, also while
    /// it is paused.
    pub fn wants_reports_when_paused(&self) -> bool {
        matches!(self, Server::Ister(_))
    }

    /// Whether the user wants intros skipped without being asked.
    pub fn skips_intros(&self) -> bool {
        match self {
            Server::Plex(_) => false,
            Server::Ister(server) => server.skips_intros(),
        }
    }

    /// What tells this server from others; also names its folder in the
    /// cache.
    pub fn machine_identifier(&self) -> &str {
        each!(self, server => server.machine_identifier())
    }

    pub fn friendly_name(&self) -> Option<&str> {
        each!(self, server => server.friendly_name())
    }

    /// The host the server is reached at, to tell the user.
    pub fn host(&self) -> String {
        match self {
            Server::Plex(server) => server.base_url().host_str().unwrap_or_default().to_owned(),
            Server::Ister(server) => reqwest::Url::parse(server.api_url())
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_default(),
        }
    }

    // Libraries

    pub async fn sections(&self) -> Result<Vec<LibrarySection>> {
        each!(self, server => server.sections().await)
    }

    pub async fn recently_added(&self) -> Result<Vec<Item>> {
        each!(self, server => server.recently_added().await)
    }

    pub async fn on_deck(&self) -> Result<Vec<Item>> {
        each!(self, server => server.on_deck().await)
    }

    pub async fn section_on_deck(&self, section: ItemId) -> Result<Vec<Item>> {
        each!(self, server => server.section_on_deck(section).await)
    }

    pub async fn section_items(
        &self,
        section: ItemId,
        offset: u32,
        size: u32,
        sort: Option<&str>,
    ) -> Result<Page<Item>> {
        each!(self, server => server.section_items(section, offset, size, sort).await)
    }

    pub async fn section_albums(&self, section: ItemId) -> Result<Vec<Item>> {
        each!(self, server => server.section_albums(section).await)
    }

    pub async fn section_sorts(&self, section: ItemId) -> Result<Vec<SortOption>> {
        each!(self, server => server.section_sorts(section).await)
    }

    // Items

    pub async fn item(&self, id: ItemId) -> Result<Item> {
        each!(self, server => server.item(id).await)
    }

    pub async fn children(&self, id: ItemId) -> Result<Vec<Item>> {
        each!(self, server => server.children(id).await)
    }

    pub async fn children_page(&self, id: ItemId, offset: u32, size: u32) -> Result<Page<Item>> {
        each!(self, server => server.children_page(id, offset, size).await)
    }

    pub async fn all_leaves(&self, id: ItemId) -> Result<Vec<Item>> {
        each!(self, server => server.all_leaves(id).await)
    }

    pub async fn playlists(&self) -> Result<Vec<Item>> {
        each!(self, server => server.playlists().await)
    }

    pub async fn playlist_items(&self, id: ItemId) -> Result<Vec<Item>> {
        each!(self, server => server.playlist_items(id).await)
    }

    pub async fn playlist_page(&self, id: ItemId, offset: u32, size: u32) -> Result<Page<Item>> {
        each!(self, server => server.playlist_page(id, offset, size).await)
    }

    pub async fn search(&self, query: &str, kind: ItemKind, limit: u32) -> Result<Vec<Item>> {
        each!(self, server => server.search(query, kind, limit).await)
    }

    // What the user did with an item

    pub async fn mark_watched(&self, id: ItemId) -> Result<()> {
        each!(self, server => server.mark_watched(id).await)
    }

    pub async fn mark_unwatched(&self, id: ItemId) -> Result<()> {
        each!(self, server => server.mark_unwatched(id).await)
    }

    /// `rating` runs from 0 to 10.
    pub async fn rate(&self, id: ItemId, rating: f32) -> Result<()> {
        each!(self, server => server.rate(id, rating).await)
    }

    pub async fn select_audio_stream(&self, part: &Part, stream: ItemId) -> Result<()> {
        each!(self, server => server.select_audio_stream(part, stream).await)
    }

    /// `None` turns subtitles off.
    pub async fn select_subtitle_stream(&self, part: &Part, stream: Option<ItemId>) -> Result<()> {
        each!(self, server => server.select_subtitle_stream(part, stream).await)
    }

    // Addresses

    /// Where to get the image `thumb` of an item, scaled by the server.
    pub fn image_url(&self, thumb: &str, width: u32, height: u32) -> String {
        each!(self, server => server.image_url(thumb, width, height).to_string())
    }

    /// Where to get a file that belongs to an item, such as subtitles kept
    /// beside the video (the `key` of the stream).
    pub fn download_url(&self, key: &str) -> String {
        each!(self, server => server.download_url(key).to_string())
    }

    /// The pictures of a subtitle stream that is made of them.
    pub async fn picture_subtitles(&self, key: &str) -> Result<PictureSubtitles> {
        match self {
            Server::Plex(_) => Err(girens_model::Error::Server(
                "this server has no subtitles as pictures".into(),
            )),
            Server::Ister(server) => server.picture_subtitles(key).await,
        }
    }

    // Playing

    /// Has the server make `item` (with its files, as [`Server::item`]
    /// gives it) ready to play.
    pub async fn open_stream(&self, item: &Item, options: &StreamOptions) -> Result<Opened> {
        match self {
            Server::Plex(server) => {
                if options.direct {
                    let part = item
                        .parts()
                        .next()
                        .ok_or(girens_model::Error::NotFound(item.rating_key))?;
                    return Ok(Opened {
                        url: server.part_url(part).to_string(),
                        session: None,
                        subtitle_delay: 0.0,
                        segmented: false,
                    });
                }
                let request = TranscodeRequest {
                    session: new_session(),
                    video_resolution: options.resolution.clone(),
                    offset: 0,
                };
                if let Err(err) = server.transcode_decision(item, &request).await {
                    eprintln!("The server did not accept the transcode request: {err}");
                }
                Ok(Opened {
                    url: server.transcode_url(item, &request).to_string(),
                    session: Some(request.session),
                    subtitle_delay: 0.0,
                    segmented: true,
                })
            }
            // The server converts by itself, without a session to look after.
            Server::Ister(server) => Ok(Opened {
                url: server.stream_url(item, options.direct)?,
                session: None,
                subtitle_delay: girens_ister::SUBTITLE_DELAY,
                segmented: true,
            }),
        }
    }

    /// Where to download `item` from, to keep a copy: the file as it is, or
    /// converted to `resolution`.
    pub fn download_source(&self, item: &Item, resolution: Option<&str>) -> Result<Download> {
        match self {
            Server::Plex(server) => match resolution {
                Some(resolution) => {
                    let request = TranscodeRequest {
                        session: new_session(),
                        video_resolution: Some(resolution.to_owned()),
                        offset: 0,
                    };
                    Ok(Download {
                        url: server.transcode_download_url(item, &request).to_string(),
                        session: Some(request.session),
                    })
                }
                None => {
                    let part = item
                        .parts()
                        .next()
                        .ok_or(girens_model::Error::NotFound(item.rating_key))?;
                    Ok(Download {
                        url: server.download_url(&part.key).to_string(),
                        session: None,
                    })
                }
            },
            Server::Ister(_) => Err(girens_model::Error::Server(
                "this server has no files to download".into(),
            )),
        }
    }

    /// Keeps the conversion of what is playing going.
    pub async fn keep_stream(&self, item: &Item, session: &str) -> Result<()> {
        match self {
            Server::Plex(server) => server.ping_transcode_session(item, session).await,
            Server::Ister(_) => Ok(()),
        }
    }

    /// Lets the server stop converting.
    pub async fn end_stream(&self, item: &Item, session: &str) -> Result<()> {
        match self {
            Server::Plex(server) => server.stop_transcode_session(item, session).await,
            Server::Ister(_) => Ok(()),
        }
    }

    /// Reports where playback of `item`, an entry of a play queue, is.
    pub async fn update_timeline(
        &self,
        item: &Item,
        position: u64,
        state: PlaybackState,
        session: Option<&str>,
    ) -> Result<()> {
        each!(self, server => server.update_timeline(item, position, state, session).await)
    }

    // Play queues

    pub async fn create_play_queue(
        &self,
        item: &Item,
        start_item: Option<&Item>,
        shuffle: bool,
    ) -> Result<PlayQueue> {
        each!(self, server => server.create_play_queue(item, start_item, shuffle).await)
    }

    pub async fn create_section_play_queue(
        &self,
        section: &LibrarySection,
        sort: Option<&str>,
        shuffle: bool,
    ) -> Result<PlayQueue> {
        each!(self, server => server.create_section_play_queue(section, sort, shuffle).await)
    }

    pub async fn refresh_play_queue(&self, id: ItemId) -> Result<PlayQueue> {
        each!(self, server => server.refresh_play_queue(id).await)
    }

    pub async fn shuffle_play_queue(&self, id: ItemId, shuffle: bool) -> Result<PlayQueue> {
        each!(self, server => server.shuffle_play_queue(id, shuffle).await)
    }
}

/// A name for a conversion session that no other player uses.
fn new_session() -> String {
    gtk::glib::uuid_string_random().to_string()
}
