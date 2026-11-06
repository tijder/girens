//! Runs the client against a mock server that answers with recorded,
//! anonymised responses of a real Plex Media Server (see
//! `tools/record_fixtures.py`).

use girens_plex::model::{ItemKind, StreamKind};
use girens_plex::playback::{PlaybackState, TranscodeRequest};
use girens_plex::{ClientInfo, Error, PlexServer};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "test-token";

fn fixture(name: &str) -> ResponseTemplate {
    let file = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let body = std::fs::read_to_string(&file).unwrap_or_else(|err| panic!("{file}: {err}"));
    ResponseTemplate::new(200).set_body_raw(body, "application/json")
}

fn client_info() -> ClientInfo {
    ClientInfo {
        identifier: "test-client".into(),
        product: "Girens".into(),
        version: "0".into(),
        device_name: "test".into(),
    }
}

async fn serve(mock: &MockServer, url_path: &str, name: &str) {
    Mock::given(method("GET"))
        .and(path(url_path))
        .respond_with(fixture(name))
        .mount(mock)
        .await;
}

async fn connect() -> (MockServer, PlexServer) {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .and(header("x-plex-token", TOKEN))
        .and(header("x-plex-client-identifier", "test-client"))
        .and(header("accept", "application/json"))
        .respond_with(fixture("server_root"))
        .mount(&mock)
        .await;
    let server = PlexServer::connect(&mock.uri(), TOKEN, &client_info())
        .await
        .expect("connect");
    (mock, server)
}

#[tokio::test]
async fn connect_reads_the_server_identity() {
    let (_mock, server) = connect().await;
    assert!(server
        .machine_identifier()
        .starts_with("machineIdentifier-"));
    assert!(server.friendly_name().is_some());
}

#[tokio::test]
async fn a_rejected_token_is_reported_as_unauthorized() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock)
        .await;
    let error = PlexServer::connect(&mock.uri(), "wrong", &client_info())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Unauthorized));
}

#[tokio::test]
async fn errors_do_not_contain_the_token() {
    let (mock, server) = connect().await;
    Mock::given(method("GET"))
        .and(path("/library/onDeck"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;
    let error = server.on_deck().await.unwrap_err();
    assert!(matches!(error, Error::Status { .. }));
    assert!(!error.to_string().contains(TOKEN));
}

#[tokio::test]
async fn sections_and_sorts() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/sections", "library_sections").await;
    serve(&mock, "/library/sections/1/sorts", "section_movie_sorts").await;

    let sections = server.sections().await.unwrap();
    assert!(!sections.is_empty());
    for kind in [ItemKind::Movie, ItemKind::Show, ItemKind::Artist] {
        assert!(sections.iter().any(|section| section.kind == kind));
    }

    let sorts = server.section_sorts(1).await.unwrap();
    assert!(sorts.iter().any(|sort| sort.key == "titleSort"));
}

#[tokio::test]
async fn a_section_page_reports_the_total() {
    let (mock, server) = connect().await;
    Mock::given(method("GET"))
        .and(path("/library/sections/1/all"))
        .and(query_param("X-Plex-Container-Start", "0"))
        .and(query_param("X-Plex-Container-Size", "3"))
        .and(query_param("sort", "titleSort:asc"))
        .respond_with(fixture("section_movie_all"))
        .mount(&mock)
        .await;

    let page = server
        .section_items(1, 0, 3, Some("titleSort:asc"))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.offset, 0);
    assert!(page.total > 3);
    assert!(page.items.iter().all(|item| item.kind == ItemKind::Movie));
}

#[tokio::test]
async fn home_lists_parse() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/recentlyAdded", "recently_added").await;
    serve(&mock, "/library/onDeck", "on_deck").await;
    serve(&mock, "/library/sections/2/onDeck", "section_show_on_deck").await;

    assert!(!server.recently_added().await.unwrap().is_empty());
    assert!(!server.on_deck().await.unwrap().is_empty());
    server.section_on_deck(2).await.unwrap();
}

#[tokio::test]
async fn show_hierarchy() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/metadata/1", "show_metadata").await;
    serve(&mock, "/library/metadata/1/children", "show_children").await;
    serve(&mock, "/library/metadata/1/allLeaves", "show_all_leaves").await;
    serve(&mock, "/library/metadata/2", "episode_metadata").await;

    let show = server.item(1).await.unwrap();
    assert_eq!(show.kind, ItemKind::Show);
    assert!(show.leaf_count.is_some());

    let seasons = server.children(1).await.unwrap();
    assert!(seasons.iter().all(|season| season.kind == ItemKind::Season));

    let episodes = server.all_leaves(1).await.unwrap();
    assert!(!episodes.is_empty());

    let episode = server.item(2).await.unwrap();
    assert_eq!(episode.kind, ItemKind::Episode);
    assert!(episode.grandparent_rating_key.is_some());
    assert!(episode.season_episode().unwrap().starts_with('s'));
    assert_eq!(episode.cover(), episode.grandparent_thumb.as_deref());
    assert!(episode.parts().next().is_some());
    assert!(episode.streams(StreamKind::Video).next().is_some());
    assert!(episode.selected_stream(StreamKind::Audio).is_some());
}

#[tokio::test]
async fn music_hierarchy() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/metadata/1", "artist_metadata").await;
    serve(&mock, "/library/metadata/1/children", "artist_children").await;
    serve(&mock, "/library/metadata/2/children", "album_children").await;
    serve(&mock, "/library/metadata/3", "track_metadata").await;
    serve(&mock, "/library/sections/5/albums", "section_artist_albums").await;

    assert_eq!(server.item(1).await.unwrap().kind, ItemKind::Artist);
    let albums = server.children(1).await.unwrap();
    assert!(albums.iter().all(|album| album.kind == ItemKind::Album));
    let tracks = server.children(2).await.unwrap();
    assert!(tracks.iter().all(|track| track.kind == ItemKind::Track));
    let track = server.item(3).await.unwrap();
    assert!(track.kind.is_audio());
    assert!(track.parent_rating_key.is_some());
    assert!(!server.section_albums(5).await.unwrap().is_empty());
}

#[tokio::test]
async fn playlists_use_their_composite_as_cover() {
    let (mock, server) = connect().await;
    serve(&mock, "/playlists", "playlists").await;
    serve(&mock, "/playlists/9/items", "playlist_items").await;

    let playlists = server.playlists().await.unwrap();
    let playlist = &playlists[0];
    assert_eq!(playlist.kind, ItemKind::Playlist);
    assert_eq!(playlist.cover(), playlist.composite.as_deref());
    assert!(!server.playlist_items(9).await.unwrap().is_empty());
    let page = server.playlist_page(9, 0, 50).await.unwrap();
    assert!(!page.items.is_empty());
    assert!(page.total >= page.items.len() as u32);
}

#[tokio::test]
async fn search_filters_on_type() {
    let (mock, server) = connect().await;
    Mock::given(method("GET"))
        .and(path("/library/all"))
        .and(query_param("title", "abc"))
        .and(query_param("type", "1"))
        .and(query_param("X-Plex-Container-Size", "10"))
        .respond_with(fixture("search_movie"))
        .mount(&mock)
        .await;
    let results = server.search("abc", ItemKind::Movie, 10).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn a_missing_item_is_not_found() {
    let (mock, server) = connect().await;
    Mock::given(method("GET"))
        .and(path("/library/metadata/404"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"MediaContainer":{"size":0}}"#, "application/json"),
        )
        .mount(&mock)
        .await;
    assert!(matches!(
        server.item(404).await.unwrap_err(),
        Error::NotFound(404)
    ));
}

#[tokio::test]
async fn watched_state_and_rating_requests() {
    let (mock, server) = connect().await;
    for (verb, url_path) in [("GET", "/:/scrobble"), ("GET", "/:/unscrobble")] {
        Mock::given(method(verb))
            .and(path(url_path))
            .and(query_param("key", "42"))
            .and(query_param("identifier", "com.plexapp.plugins.library"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock)
            .await;
    }
    Mock::given(method("PUT"))
        .and(path("/:/rate"))
        .and(query_param("key", "42"))
        .and(query_param("rating", "8"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock)
        .await;

    server.mark_watched(42).await.unwrap();
    server.mark_unwatched(42).await.unwrap();
    server.rate(42, 8.0).await.unwrap();
}

#[tokio::test]
async fn playback_urls_and_timeline() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/metadata/2", "episode_metadata").await;
    let episode = server.item(2).await.unwrap();
    let part = episode.parts().next().unwrap();

    let direct = server.part_url(part);
    assert_eq!(direct.path(), part.key);
    assert!(direct.query().unwrap().contains("X-Plex-Token=test-token"));

    let image = server.image_url(episode.thumb.as_deref().unwrap(), 300, 200);
    assert_eq!(image.path(), "/photo/:/transcode");
    let pairs: Vec<_> = image.query_pairs().collect();
    assert!(pairs
        .iter()
        .any(|(k, v)| k == "url" && v == episode.thumb.as_deref().unwrap()));
    assert!(pairs.iter().any(|(k, v)| k == "width" && v == "300"));

    let request = TranscodeRequest {
        session: "session-1".into(),
        video_resolution: Some("1280x720".into()),
        offset: 90,
    };
    let stream = server.transcode_url(&episode, &request);
    assert_eq!(stream.path(), "/video/:/transcode/universal/start.mpd");
    let pairs: Vec<_> = stream.query_pairs().collect();
    for (name, value) in [
        ("protocol", "dash"),
        ("session", "session-1"),
        ("videoResolution", "1280x720"),
        ("offset", "90"),
        ("path", episode.key.as_str()),
    ] {
        assert!(pairs.iter().any(|(k, v)| k == name && v == value), "{name}");
    }

    Mock::given(method("GET"))
        .and(path("/video/:/transcode/universal/decision"))
        .and(query_param("session", "session-1"))
        .and(query_param("protocol", "dash"))
        .and(query_param("subtitles", "burn"))
        .and(header("x-plex-platform", "Chrome"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("{}", "application/json"))
        .expect(1)
        .mount(&mock)
        .await;
    server.transcode_decision(&episode, &request).await.unwrap();

    Mock::given(method("GET"))
        .and(path("/:/timeline"))
        .and(query_param("ratingKey", episode.rating_key.to_string()))
        .and(query_param("time", "5000"))
        .and(query_param("state", "playing"))
        .and(query_param("X-Plex-Session-Identifier", "session-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock)
        .await;
    server
        .update_timeline(&episode, 5000, PlaybackState::Playing, Some("session-1"))
        .await
        .unwrap();
}

#[tokio::test]
async fn an_invalid_resolution_is_left_out() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/metadata/2", "episode_metadata").await;
    let episode = server.item(2).await.unwrap();
    let request = TranscodeRequest {
        session: "s".into(),
        video_resolution: Some("original".into()),
        offset: 0,
    };
    let url = server.transcode_url(&episode, &request);
    assert!(!url.query_pairs().any(|(name, _)| name == "videoResolution"));
}

#[tokio::test]
async fn a_base_url_with_a_path_prefix_is_kept() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/plex"))
        .respond_with(fixture("server_root"))
        .mount(&mock)
        .await;
    let server = PlexServer::connect(&format!("{}/plex", mock.uri()), TOKEN, &client_info())
        .await
        .unwrap();
    assert_eq!(
        server.url("/library/sections", &[], false).path(),
        "/plex/library/sections"
    );
}

#[tokio::test]
async fn a_download_is_written_completely_or_not_at_all() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/file"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 100_000]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;
    let dir = std::env::temp_dir().join(format!("girens-plex-test-{}", std::process::id()));
    let target = dir.join("item_1");

    let missing = girens_plex::download_to_file(&format!("{}/missing", mock.uri()), &target).await;
    assert!(matches!(missing, Err(Error::Status { .. })));
    assert!(!target.exists());

    girens_plex::download_to_file(&format!("{}/file", mock.uri()), &target)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), vec![7u8; 100_000]);
    assert!(!dir.join("item_1.tmp").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_interrupted_download_is_continued() {
    let body: Vec<u8> = (0..=255u8).cycle().take(10_000).collect();
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/file"))
        .and(header("range", "bytes=4000-"))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(body[4000..].to_vec()))
        .expect(1)
        .mount(&mock)
        .await;
    let dir = std::env::temp_dir().join(format!("girens-plex-resume-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("item_2");
    // What an earlier attempt left behind.
    std::fs::write(dir.join("item_2.tmp"), &body[..4000]).unwrap();

    let reported = std::sync::Arc::new(std::sync::Mutex::new((0u64, None)));
    let seen = reported.clone();
    girens_plex::download_to_file_with_progress(
        &format!("{}/file", mock.uri()),
        &target,
        true,
        move |received, total| *seen.lock().unwrap() = (received, total),
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), body);
    assert_eq!(*reported.lock().unwrap(), (10_000, Some(10_000)));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_item_survives_being_stored_and_read_back() {
    let (mock, server) = connect().await;
    serve(&mock, "/library/metadata/2", "episode_metadata").await;
    let episode = server.item(2).await.unwrap();

    let stored = serde_json::to_string(&episode).unwrap();
    let restored: girens_plex::model::Item = serde_json::from_str(&stored).unwrap();

    assert_eq!(restored.rating_key, episode.rating_key);
    assert_eq!(restored.kind, ItemKind::Episode);
    assert_eq!(restored.title, episode.title);
    assert_eq!(
        restored.grandparent_rating_key,
        episode.grandparent_rating_key
    );
    assert_eq!(restored.season_episode(), episode.season_episode());
    assert_eq!(restored.parts().count(), episode.parts().count());
    assert_eq!(
        restored.selected_stream(StreamKind::Audio).map(|s| s.id),
        episode.selected_stream(StreamKind::Audio).map(|s| s.id)
    );
}
