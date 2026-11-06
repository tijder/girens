//! The GraphQL documents sent to the server.
//!
//! Each is a complete document: the operation followed by the fragments it
//! uses. The tests check all of them against the server's schema.

macro_rules! document {
    ($operation:expr $(, $fragment:ident)*) => {
        concat!($operation $(, "\n", $fragment!())*)
    };
}

macro_rules! image {
    () => {
        "fragment Img on Image { id type selected directory { servingNode { url } } }"
    };
}
macro_rules! metadata {
    () => {
        "fragment Meta on Metadata { language title description released genre }"
    };
}
macro_rules! status {
    () => {
        "fragment Status on WatchStatus { id watched progressInMilliseconds updatedAt }"
    };
}
macro_rules! file {
    () => {
        "fragment File on MediaFile { id size durationInMilliseconds format \
         directory { servingNode { url } } \
         mediaFileStreams { id streamIndex codecName codecType width height language title } \
         segments { type startInMilliseconds endInMilliseconds episodeId } }"
    };
}
/// A file as far as a list needs it: how long it plays. What it holds is
/// asked for when the item itself is.
macro_rules! file_brief {
    () => {
        "fragment File on MediaFile { id durationInMilliseconds }"
    };
}
macro_rules! movie {
    () => {
        "fragment MovieF on Movie { id name releaseYear rating images { ...Img } \
         metadata { ...Meta } watchStatus { ...Status } mediaFile { ...File } }"
    };
}
macro_rules! show {
    () => {
        "fragment ShowF on Show { id name releaseYear rating images { ...Img } metadata { ...Meta } }"
    };
}
macro_rules! episode {
    () => {
        "fragment EpisodeF on Episode { id number rating season { id number } \
         show { id name images { ...Img } } images { ...Img } metadata { ...Meta } \
         watchStatus { ...Status } mediaFile { ...File } \
         mediaFileParts { startInMilliseconds durationInMilliseconds mediaFile { id } } }"
    };
}
macro_rules! artist {
    () => {
        "fragment ArtistF on Person { id name images { ...Img } metadata { ...Meta } }"
    };
}
macro_rules! album {
    () => {
        "fragment AlbumF on Album { id name releaseYear rating artist { id name } images { ...Img } }"
    };
}
macro_rules! track {
    () => {
        "fragment TrackF on Track { id number discNumber rating playCount artist { id name } \
         album { id name images { ...Img } } metadata { ...Meta } mediaFile { ...File } }"
    };
}
macro_rules! book {
    () => {
        "fragment BookF on Book { id title releaseYear rating author { id name } \
         images { ...Img } chapters { id } }"
    };
}
macro_rules! chapter {
    () => {
        "fragment ChapterF on Chapter { id number author { id name } \
         book { id title images { ...Img } } metadata { ...Meta } \
         watchStatus { ...Status } mediaFile { ...File } }"
    };
}
macro_rules! podcast {
    () => {
        "fragment PodcastF on Podcast { id title author rating images { ...Img } }"
    };
}
macro_rules! podcast_episode {
    () => {
        "fragment PodcastEpisodeF on PodcastEpisode { id episodeNumber downloaded \
         durationInMilliseconds podcast { id title author images { ...Img } } \
         metadata { ...Meta } watchStatus { ...Status } mediaFile { ...File } }"
    };
}
macro_rules! queue {
    () => {
        "fragment Queue on PlayQueue { id currentItemId shuffle \
         playQueueItems { id type movie { ...MovieF } episode { ...EpisodeF } track { ...TrackF } \
         chapter { ...ChapterF } podcastEpisode { ...PodcastEpisodeF } } }"
    };
}

pub const ME: &str = "query Me { me { id name email } }";

pub const LIBRARIES: &str = "query Libraries { libraries { id name type } }";

pub const USER_SETTINGS: &str = "query UserSettings { userSettings { \
     preferredAudioLanguages preferredSubtitleLanguages hideSubtitlesMatchingAudio autoSkipIntro } }";

pub const CREATE_STREAM_TOKEN: &str = "mutation CreateStreamToken { createStreamToken { token } }";

pub const MOVIES: &str = document!(
    "query Movies($library: ID, $page: Int, $size: Int, $sorting: SortingEnum, $order: SortingOrder) { \
     movies(libraryId: $library, page: $page, size: $size, sorting: $sorting, sortingOrder: $order) { \
     totalElements content { ...MovieF } } }",
    movie,
    image,
    metadata,
    status,
    file_brief
);

pub const SHOWS: &str = document!(
    "query Shows($library: ID, $page: Int, $size: Int, $sorting: SortingEnum, $order: SortingOrder) { \
     shows(libraryId: $library, page: $page, size: $size, sorting: $sorting, sortingOrder: $order) { \
     totalElements content { ...ShowF } } }",
    show,
    image,
    metadata
);

pub const ARTISTS: &str = document!(
    "query Artists($library: ID, $page: Int, $size: Int, $sorting: SortingEnum, $order: SortingOrder) { \
     persons(libraryId: $library, page: $page, size: $size, sorting: $sorting, sortingOrder: $order) { \
     totalElements content { ...ArtistF } } }",
    artist,
    image,
    metadata
);

pub const ALBUMS: &str = document!(
    "query Albums($library: ID, $artist: ID, $page: Int, $size: Int, $sorting: SortingEnum, $order: SortingOrder) { \
     albums(libraryId: $library, artistId: $artist, page: $page, size: $size, sorting: $sorting, sortingOrder: $order) { \
     totalElements content { ...AlbumF } } }",
    album,
    image
);

pub const TRACKS: &str = document!(
    "query Tracks($artist: ID, $page: Int, $size: Int) { \
     tracks(artistId: $artist, page: $page, size: $size) { totalElements content { ...TrackF } } }",
    track,
    image,
    metadata,
    file_brief
);

pub const MOVIE: &str = document!(
    "query Movie($id: ID) { movieById(id: $id) { ...MovieF } }",
    movie,
    image,
    metadata,
    status,
    file
);

/// A show with what tells how much of it was watched.
pub const SHOW: &str = document!(
    "query Show($id: ID) { showById(id: $id) { ...ShowF \
     seasons { id number episodes { id watchStatus { watched } } } } }",
    show,
    image,
    metadata
);

pub const SHOW_EPISODES: &str = document!(
    "query ShowEpisodes($id: ID) { showById(id: $id) { episodes { ...EpisodeF } } }",
    episode,
    image,
    metadata,
    status,
    file_brief
);

pub const SEASON: &str = document!(
    "query Season($id: ID) { seasonById(id: $id) { id number show { ...ShowF } \
     episodes { ...EpisodeF } } }",
    show,
    episode,
    image,
    metadata,
    status,
    file_brief
);

pub const EPISODE: &str = document!(
    "query Episode($id: ID) { episodeById(id: $id) { ...EpisodeF } }",
    episode,
    image,
    metadata,
    status,
    file
);

pub const ARTIST: &str = document!(
    "query Artist($id: ID) { personById(id: $id) { ...ArtistF albums { ...AlbumF } } }",
    artist,
    album,
    image,
    metadata
);

pub const ALBUM: &str = document!(
    "query Album($id: ID) { albumById(id: $id) { ...AlbumF tracks { ...TrackF } } }",
    album,
    track,
    image,
    metadata,
    file_brief
);

pub const TRACK: &str = document!(
    "query Track($id: ID) { trackById(id: $id) { ...TrackF } }",
    track,
    image,
    metadata,
    file
);

pub const BOOKS: &str = document!(
    "query Books($library: ID, $page: Int, $size: Int, $sorting: SortingEnum, $order: SortingOrder) { \
     books(libraryId: $library, page: $page, size: $size, sorting: $sorting, sortingOrder: $order) { \
     totalElements content { ...BookF } } }",
    book,
    image
);

pub const BOOK: &str = document!(
    "query Book($id: ID) { bookById(id: $id) { ...BookF chapters { ...ChapterF } } }",
    book,
    chapter,
    image,
    metadata,
    status,
    file_brief
);

pub const CHAPTER: &str = document!(
    "query Chapter($id: ID) { chapterById(id: $id) { ...ChapterF } }",
    chapter,
    image,
    metadata,
    status,
    file
);

pub const PODCASTS: &str = document!(
    "query Podcasts($library: ID, $page: Int, $size: Int, $sorting: SortingEnum, $order: SortingOrder) { \
     podcasts(libraryId: $library, page: $page, size: $size, sorting: $sorting, sortingOrder: $order) { \
     totalElements content { ...PodcastF } } }",
    podcast,
    image
);

pub const PODCAST: &str = document!(
    "query Podcast($id: ID) { podcastById(id: $id) { ...PodcastF } }",
    podcast,
    image
);

pub const PODCAST_EPISODES: &str = document!(
    "query PodcastEpisodes($podcast: ID!, $page: Int, $size: Int) { \
     podcastEpisodes(podcastId: $podcast, page: $page, size: $size) { \
     totalElements content { ...PodcastEpisodeF } } }",
    podcast_episode,
    image,
    metadata,
    status,
    file_brief
);

pub const PODCAST_EPISODE: &str = document!(
    "query PodcastEpisode($id: ID) { podcastEpisodeById(id: $id) { ...PodcastEpisodeF } }",
    podcast_episode,
    image,
    metadata,
    status,
    file
);

pub const RECENTLY_WATCHED: &str = document!(
    "query RecentlyWatched($library: ID) { recentlyWatched(libraryId: $library) { \
     type movie { ...MovieF } episode { ...EpisodeF } chapter { ...ChapterF } \
     podcastEpisode { ...PodcastEpisodeF } } }",
    chapter,
    podcast_episode,
    movie,
    episode,
    image,
    metadata,
    status,
    file_brief
);

pub const SEARCH: &str = document!(
    "query Search($term: String!, $size: Int) { search(term: $term, size: $size) { __typename \
     ... on Movie { ...MovieF } ... on Show { ...ShowF } ... on Episode { ...EpisodeF } \
     ... on Person { ...ArtistF } ... on Album { ...AlbumF } ... on Track { ...TrackF } } }",
    movie,
    show,
    episode,
    artist,
    album,
    track,
    image,
    metadata,
    status,
    file_brief
);

pub const PLAYLISTS: &str = document!(
    "query Playlists { playlists { id name type libraryType itemCount coverImages { ...Img } } }",
    image
);

pub const PLAYLIST: &str = document!(
    "query Playlist($id: ID!) { playlistById(id: $id) { id name type libraryType itemCount \
     coverImages { ...Img } \
     items { id type movie { ...MovieF } episode { ...EpisodeF } track { ...TrackF } } } }",
    movie,
    episode,
    track,
    image,
    metadata,
    status,
    file_brief
);

pub const CREATE_PLAY_QUEUE: &str = document!(
    "mutation CreatePlayQueue($input: CreatePlayQueueInput!) { createPlayQueue(input: $input) { ...Queue } }",
    queue,
    chapter,
    podcast_episode,
    movie,
    episode,
    track,
    image,
    metadata,
    status,
    file_brief
);

pub const GET_PLAY_QUEUE: &str = document!(
    "query GetPlayQueue($id: ID!) { getPlayQueue(id: $id) { ...Queue } }",
    queue,
    chapter,
    podcast_episode,
    movie,
    episode,
    track,
    image,
    metadata,
    status,
    file_brief
);

/// Reports the position; also what keeps the session of the player alive.
pub const UPDATE_PLAY_QUEUE: &str = "mutation UpdatePlayQueue($id: ID!, $item: ID!, $progress: Int!, \
     $settings: StreamSettingsInput, $state: PlayState, $device: ID) { \
     updatePlayQueue(id: $id, playQueueItemId: $item, progressInMilliseconds: $progress, \
     streamSettings: $settings, playState: $state, deviceId: $device) { id } }";

pub const MARK_PLAYED: &str = "mutation MarkPlayed($type: MediaType!, $id: ID!) { \
     markPlayed(mediaType: $type, mediaId: $id) { id } }";

pub const PLAYBACK_HISTORY: &str = "query PlaybackHistory($type: MediaType!, $id: ID!) { \
     playbackHistory(mediaType: $type, mediaId: $id) { id } }";

pub const DELETE_WATCH_STATUS: &str =
    "mutation DeleteWatchStatus($id: ID!) { deleteWatchStatus(id: $id) }";

pub const SET_RATING: &str = "mutation SetRating($type: RatingMediaType!, $id: ID!, $rating: Int) { \
     setRating(mediaType: $type, mediaId: $id, rating: $rating) }";

/// Every document, for the tests.
pub const ALL: &[&str] = &[
    ME,
    LIBRARIES,
    USER_SETTINGS,
    CREATE_STREAM_TOKEN,
    MOVIES,
    SHOWS,
    ARTISTS,
    ALBUMS,
    TRACKS,
    MOVIE,
    SHOW,
    SHOW_EPISODES,
    SEASON,
    EPISODE,
    ARTIST,
    ALBUM,
    TRACK,
    BOOKS,
    BOOK,
    CHAPTER,
    PODCASTS,
    PODCAST,
    PODCAST_EPISODES,
    PODCAST_EPISODE,
    RECENTLY_WATCHED,
    SEARCH,
    PLAYLISTS,
    PLAYLIST,
    CREATE_PLAY_QUEUE,
    GET_PLAY_QUEUE,
    UPDATE_PLAY_QUEUE,
    MARK_PLAYED,
    PLAYBACK_HISTORY,
    DELETE_WATCH_STATUS,
    SET_RATING,
];
