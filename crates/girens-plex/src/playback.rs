//! Transcoding and progress reporting.

use reqwest::Method;
use url::Url;

use crate::model::Item;
use crate::server::LIBRARY_IDENTIFIER;
use crate::{PlexServer, Result};

/// The client profile the transcoder uses for streams played by Girens.
const TRANSCODE_PLATFORM: &str = "Chrome";

/// What to ask the transcoder for.
#[derive(Debug, Clone)]
pub struct TranscodeRequest {
    /// Identifies the transcode session; a fresh UUID per playback.
    pub session: String,
    /// `1920x1080`, `1280x720`, … Anything else is ignored.
    pub video_resolution: Option<String>,
    /// Start position in seconds.
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackState {
    fn as_str(self) -> &'static str {
        match self {
            PlaybackState::Playing => "playing",
            PlaybackState::Paused => "paused",
            PlaybackState::Stopped => "stopped",
        }
    }
}

fn stream_type(item: &Item) -> &'static str {
    if item.kind.is_audio() {
        "audio"
    } else {
        "video"
    }
}

fn valid_resolution(resolution: Option<&str>) -> Option<&str> {
    resolution.filter(|value| {
        matches!(value.split_once('x'), Some((width, height))
            if !width.is_empty() && !height.is_empty()
                && width.bytes().all(|b| b.is_ascii_digit())
                && height.bytes().all(|b| b.is_ascii_digit()))
    })
}

impl PlexServer {
    /// Asks the server to decide how it will transcode `item`. Has to be
    /// called before the DASH stream of the same session is opened.
    pub async fn transcode_decision(&self, item: &Item, request: &TranscodeRequest) -> Result<()> {
        let mut params = vec![
            ("hasMDE", "1"),
            ("path", item.key.as_str()),
            ("session", request.session.as_str()),
            ("protocol", "dash"),
            ("directPlay", "0"),
            ("directStream", "1"),
            ("fastSeek", "1"),
            ("mediaIndex", "0"),
            ("partIndex", "0"),
            ("location", "lan"),
            ("autoAdjustQuality", "0"),
            ("directStreamAudio", "1"),
            ("subtitles", "burn"),
            ("subtitleSize", "100"),
            ("copyts", "1"),
            ("audioBoost", "100"),
            ("X-Plex-Platform", TRANSCODE_PLATFORM),
        ];
        if let Some(resolution) = valid_resolution(request.video_resolution.as_deref()) {
            params.push(("videoResolution", resolution));
        }
        let path = format!("/{}/:/transcode/universal/decision", stream_type(item));
        // The transcoder picks its client profile from the platform. mpv opens
        // the stream without our headers, so only the `X-Plex-Platform` query
        // parameter counts there; the decision has to be made for the same
        // platform. With the default "Linux" header the server answers 400,
        // which the Python version silently ignored.
        self.request_as_platform::<serde::de::IgnoredAny>(TRANSCODE_PLATFORM, &path, &params)
            .await
            .map(drop)
    }

    /// The DASH manifest mpv plays for a transcoded item.
    pub fn transcode_url(&self, item: &Item, request: &TranscodeRequest) -> Url {
        let offset = request.offset.to_string();
        let mut params = vec![
            ("path", item.key.as_str()),
            ("mediaIndex", "0"),
            ("partIndex", "0"),
            ("protocol", "dash"),
            ("fastSeek", "1"),
            ("copyts", "1"),
            ("offset", offset.as_str()),
            ("X-Plex-Platform", TRANSCODE_PLATFORM),
            ("session", request.session.as_str()),
        ];
        if let Some(resolution) = valid_resolution(request.video_resolution.as_deref()) {
            params.push(("videoResolution", resolution));
        }
        let path = format!("/{}/:/transcode/universal/start.mpd", stream_type(item));
        self.url(&path, &params, true)
    }

    /// A transcoded version of `item` as one Matroska stream, to download
    /// for offline use. End the session with [`Self::stop_transcode_session`].
    pub fn transcode_download_url(&self, item: &Item, request: &TranscodeRequest) -> Url {
        let mut params = vec![
            ("path", item.key.as_str()),
            ("mediaIndex", "0"),
            ("partIndex", "0"),
            ("protocol", "http"),
            ("directPlay", "0"),
            ("directStream", "1"),
            ("X-Plex-Platform", TRANSCODE_PLATFORM),
            ("session", request.session.as_str()),
        ];
        if let Some(resolution) = valid_resolution(request.video_resolution.as_deref()) {
            params.push(("videoResolution", resolution));
        }
        let path = format!("/{}/:/transcode/universal/start", stream_type(item));
        self.url(&path, &params, true)
    }

    pub async fn stop_transcode_session(&self, item: &Item, session: &str) -> Result<()> {
        let path = format!("/{}/:/transcode/universal/stop", stream_type(item));
        self.request::<serde::de::IgnoredAny>(Method::GET, &path, &[("session", session)])
            .await
            .map(drop)
    }

    /// Keeps a transcode session alive; call every 30 seconds.
    pub async fn ping_transcode_session(&self, item: &Item, session: &str) -> Result<()> {
        let path = format!("/{}/:/transcode/universal/ping", stream_type(item));
        self.request::<serde::de::IgnoredAny>(Method::GET, &path, &[("session", session)])
            .await
            .map(drop)
    }

    /// Reports the playback position (milliseconds) so the server can track
    /// resume points, watched state and "now playing".
    pub async fn update_timeline(
        &self,
        item: &Item,
        position: u64,
        state: PlaybackState,
        session: Option<&str>,
    ) -> Result<()> {
        let rating_key = item.rating_key.to_string();
        let position = position.to_string();
        let duration = item.duration.unwrap_or(0).to_string();
        let play_queue_item = item.play_queue_item_id.map(|id| id.to_string());
        let mut params = vec![
            ("ratingKey", rating_key.as_str()),
            ("key", item.key.as_str()),
            ("identifier", LIBRARY_IDENTIFIER),
            ("time", position.as_str()),
            ("state", state.as_str()),
            ("duration", duration.as_str()),
        ];
        if let Some(id) = &play_queue_item {
            params.push(("playQueueItemID", id));
        }
        if let Some(session) = session {
            params.push(("X-Plex-Provider-Version", "5.1"));
            params.push(("X-Plex-Session-Identifier", session));
        }
        self.request::<serde::de::IgnoredAny>(Method::GET, "/:/timeline", &params)
            .await
            .map(drop)
    }
}
