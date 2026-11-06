//! The client against a stand-in server that answers with what a real one
//! said (`fixtures/`, renewed with `ISTER_RECORD`; see `live.rs`).

use std::sync::{Arc, Mutex};

use girens_ister::auth::{self, Login, Provider};
use girens_ister::IsterServer;
use girens_model::model::{ItemKind, StreamKind};
use girens_model::{Error, PlaybackState};
use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// The operation a GraphQL request is for: the second word of its document.
fn operation(request: &Request) -> String {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    body["query"]
        .as_str()
        .unwrap()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .nth(1)
        .unwrap()
        .to_owned()
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

/// A provider and a server in one. `expires_in` is how long the access
/// tokens it hands out last.
async fn serve(expires_in: u64) -> MockServer {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/ister"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "Test server\n{0}/openid\n{0}/api\n",
            mock.uri()
        )))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/openid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authorization_endpoint": format!("{}/auth", mock.uri()),
            "token_endpoint": format!("{}/token", mock.uri()),
            "revocation_endpoint": format!("{}/revoke", mock.uri()),
        })))
        .mount(&mock)
        .await;
    // Every use of a refresh token gives the next one.
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(move |request: &Request| {
            let form = String::from_utf8_lossy(&request.body).into_owned();
            let number = form
                .split('&')
                .find_map(|pair| pair.strip_prefix("refresh_token=refresh-"))
                .and_then(|number| number.parse::<u32>().ok());
            let next = match number {
                Some(number) => format!("refresh-{}", number + 1),
                None if form.contains("code=the-code") => "refresh-from-code".to_owned(),
                None => {
                    return ResponseTemplate::new(400)
                        .set_body_json(json!({"error": "invalid_grant"}))
                }
            };
            ResponseTemplate::new(200).set_body_json(json!({
                "access_token": format!("access-for-{next}"),
                "refresh_token": next,
                "expires_in": expires_in,
            }))
        })
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(|request: &Request| {
            ResponseTemplate::new(200)
                .set_body_raw(fixture(&operation(request)), "application/json")
        })
        // After the answers a test sets up for itself.
        .with_priority(10)
        .mount(&mock)
        .await;
    mock
}

type Kept = Arc<Mutex<Vec<String>>>;

async fn connect(mock: &MockServer) -> (IsterServer, Kept) {
    let http = reqwest::Client::new();
    let well_known = auth::discover(&http, &mock.uri()).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    let tokens = auth::refresh(&http, &provider, "refresh-0").await.unwrap();
    let kept = Kept::default();
    let keeper = {
        let kept = kept.clone();
        Arc::new(move |token: &str| kept.lock().unwrap().push(token.to_owned()))
    };
    let server = IsterServer::connect(
        http,
        well_known,
        provider,
        tokens,
        girens_ister::Player {
            languages: vec!["nld".into()],
            labels: girens_ister::Labels::default(),
            device: "6f1c2a0e-0000-4000-8000-00000000abcd".into(),
        },
        keeper,
    )
    .await
    .unwrap();
    (server, kept)
}

#[tokio::test]
async fn connecting_tells_who_and_where() {
    let mock = serve(300).await;
    let (server, kept) = connect(&mock).await;
    assert_eq!(server.user().name, "Ister Admin");
    assert_eq!(server.friendly_name(), Some("Test server"));
    assert!(server.machine_identifier().starts_with("ister_127.0.0.1_"));
    assert!(server.machine_identifier().ends_with("_api"));
    assert_eq!(server.api_url(), format!("{}/api", mock.uri()));
    // The token was fresh: nothing new to keep.
    assert!(kept.lock().unwrap().is_empty());
    assert_eq!(server.refresh_token().await, "refresh-1");
}

#[tokio::test]
async fn an_address_that_is_no_ister_server_is_refused() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/ister"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>hello</html>"))
        .mount(&mock)
        .await;
    let http = reqwest::Client::new();
    assert!(matches!(
        auth::discover(&http, &mock.uri()).await,
        Err(Error::Server(_))
    ));
}

#[tokio::test]
async fn an_expired_token_is_renewed_and_the_new_one_kept() {
    let mock = serve(0).await;
    let (server, kept) = connect(&mock).await;
    server.sections().await.unwrap();
    // Each request found the token expired and traded the refresh token
    // for the next, which was handed over to be kept.
    let kept = kept.lock().unwrap().clone();
    assert_eq!(kept.first().map(String::as_str), Some("refresh-2"));
    assert_eq!(kept.last(), Some(&server.refresh_token().await));
    assert!(kept.len() >= 4);
}

#[tokio::test]
async fn a_refresh_token_that_ran_out_means_signing_in_again() {
    let mock = serve(300).await;
    let http = reqwest::Client::new();
    let well_known = auth::discover(&http, &mock.uri()).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    assert!(matches!(
        auth::refresh(&http, &provider, "long gone").await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn the_browser_brings_the_sign_in_back() {
    let mock = serve(300).await;
    let http = reqwest::Client::new();
    let well_known = auth::discover(&http, &mock.uri()).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    let login = Login::start(provider, true, Default::default()).await.unwrap();

    let page = url::Url::parse(&login.url).unwrap();
    let asked: std::collections::HashMap<String, String> = page.query_pairs().into_owned().collect();
    assert_eq!(asked["client_id"], "ister");
    assert_eq!(asked["code_challenge_method"], "S256");
    assert_eq!(asked["prompt"], "login");
    let redirect = asked["redirect_uri"].clone();
    assert!(redirect.starts_with("http://localhost:"));

    // What the browser does after the user signed in: first something
    // else, then an answer to another question, then the real one.
    let browser = {
        let (http, state) = (http.clone(), asked["state"].clone());
        let redirect = redirect.replace("localhost", "127.0.0.1");
        async move {
            let icon = http.get(format!("{redirect}/favicon.ico")).send().await.unwrap();
            assert_eq!(icon.status(), 404);
            let answer = http
                .get(format!("{redirect}/?code=the-code&state={state}"))
                .send()
                .await
                .unwrap();
            assert!(answer.text().await.unwrap().contains("signed in"));
        }
    };
    let (tokens, ()) = tokio::join!(login.finish(&http), browser);
    let tokens = tokens.unwrap();
    assert_eq!(tokens.refresh, "refresh-from-code");
    assert!(tokens.is_fresh());

    // The provider was given what proves this program asked.
    let exchange = mock
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|request| String::from_utf8_lossy(&request.body).contains("authorization_code"))
        .unwrap();
    let form = String::from_utf8_lossy(&exchange.body).into_owned();
    assert!(form.contains("code_verifier="));
    assert!(form.contains("client_id=ister"));
}

#[tokio::test]
async fn an_answer_to_another_question_is_refused() {
    let mock = serve(300).await;
    let http = reqwest::Client::new();
    let well_known = auth::discover(&http, &mock.uri()).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    let login = Login::start(provider, false, Default::default()).await.unwrap();
    assert!(!login.url.contains("prompt="));
    let redirect = url::Url::parse(&login.url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "redirect_uri")
        .unwrap()
        .1
        .replace("localhost", "127.0.0.1");
    let browser = async {
        let _ = http
            .get(format!("{redirect}/?code=the-code&state=someone-else"))
            .send()
            .await;
    };
    let (tokens, ()) = tokio::join!(login.finish(&http), browser);
    assert!(matches!(tokens, Err(Error::Unauthorized)));
}

#[tokio::test]
async fn only_the_libraries_girens_shows_are_listed() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let sections = server.sections().await.unwrap();
    let kinds: Vec<ItemKind> = sections.iter().map(|section| section.kind).collect();
    // The fixture also has a library of books.
    assert_eq!(kinds, [ItemKind::Show, ItemKind::Movie, ItemKind::Artist]);
    assert!(sections[0].key.as_str().starts_with("show-"));
    assert_eq!(sections[0].key.as_str(), format!("show-{}", sections[0].uuid));
}

#[tokio::test]
async fn a_library_is_asked_for_by_page_and_order() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let section = server.sections().await.unwrap().remove(1);
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_partial_json(json!({"variables": {
            "library": section.uuid, "page": 2, "size": 50,
            "sorting": "DATE_CREATED", "order": "DESCENDING",
        }})))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture("Movies"), "application/json"))
        .expect(1)
        .mount(&mock)
        .await;
    let page = server
        .section_items(section.key, 100, 50, Some("addedAt:desc"))
        .await
        .unwrap();
    assert_eq!(page.offset, 100);
    assert!(page.total >= page.items.len() as u32);
    let movie = &page.items[0];
    assert_eq!(movie.kind, ItemKind::Movie);
    assert!(movie.rating_key.as_str().starts_with("movie-"));
    assert!(movie.duration.is_some());
    // A list tells how long an item plays, not what its file holds; that
    // comes with the item itself.
    assert_eq!(movie.parts().count(), 0);
    let movie = server.item(movie.rating_key).await.unwrap();
    let part = movie.parts().next().unwrap();
    assert!(part.key.contains("/hls/"));
    assert!(movie.streams(StreamKind::Audio).next().is_some());
    // The first audio stream plays when no language says otherwise.
    assert!(movie.selected_stream(StreamKind::Audio).is_some());
}

#[tokio::test]
async fn a_show_has_seasons_and_they_have_episodes() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let show = server.item("show-any".into()).await.unwrap();
    assert_eq!(show.kind, ItemKind::Show);
    assert_eq!(show.child_count, Some(5));
    assert_eq!(show.leaf_count, Some(35));

    let seasons = server.children(show.rating_key).await.unwrap();
    assert_eq!(seasons.len(), 5);
    assert_eq!(seasons[0].kind, ItemKind::Season);
    assert_eq!(seasons[0].title, "Season 1");
    assert_eq!(seasons[0].parent_rating_key, Some(show.rating_key));
    assert_eq!(seasons[0].leaf_count, Some(7));

    let episodes = server.children(seasons[0].rating_key).await.unwrap();
    let numbers: Vec<Option<u32>> = episodes.iter().map(|episode| episode.index).collect();
    assert_eq!(numbers, (1..=7).map(Some).collect::<Vec<_>>());
    let episode = &episodes[0];
    assert_eq!(episode.season_episode().as_deref(), Some("s01e01"));
    assert_eq!(episode.grandparent_title.as_deref(), Some(show.title.as_str()));
    assert_eq!(episode.grandparent_rating_key, Some(show.rating_key));
    // An episode shows the poster of its show on a tile.
    assert_eq!(episode.cover(), show.thumb.as_deref());
}

#[tokio::test]
async fn music_runs_from_artist_to_album_to_track() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let artist = server.item("artist-any".into()).await.unwrap();
    assert_eq!(artist.kind, ItemKind::Artist);
    let albums = server.children(artist.rating_key).await.unwrap();
    assert_eq!(albums[0].kind, ItemKind::Album);
    assert_eq!(albums[0].parent_title.as_deref(), Some(artist.title.as_str()));
    let tracks = server.children(albums[0].rating_key).await.unwrap();
    assert_eq!(tracks[0].kind, ItemKind::Track);
    assert_eq!(tracks[0].index, Some(1));
    assert_eq!(tracks[0].parent_rating_key, Some(albums[0].rating_key));
    assert_eq!(tracks[0].grandparent_title.as_deref(), Some(artist.title.as_str()));
    // A track with several files plays the first.
    let track = server.item(tracks[0].rating_key).await.unwrap();
    assert_eq!(track.parts().count(), 1);
}

#[tokio::test]
async fn images_and_files_carry_the_token_for_media() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let thumb = "http://node.example/images/1/download";
    assert_eq!(
        server.image_url(thumb, 200, 300),
        format!("{thumb}?width=240&token=stream-token")
    );
    assert_eq!(server.image_url(thumb, 2000, 3000), format!("{thumb}?token=stream-token"));
    assert_eq!(
        server.download_url("http://node.example/hls/2/sub_3.srt"),
        "http://node.example/hls/2/sub_3.srt?token=stream-token"
    );
}

#[tokio::test]
async fn a_queue_is_made_played_and_reported() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let episode = server.item("episode-any".into()).await.unwrap();
    let show = episode.grandparent_rating_key.unwrap();
    let show = show.as_str().strip_prefix("show-").unwrap();
    let uuid = episode.rating_key.as_str().strip_prefix("episode-").unwrap();

    // An episode plays within its show, so that the next ones follow.
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("CreatePlayQueue"))
        .and(body_partial_json(json!({"variables": {"input": {
            "sourceType": "SHOW", "sourceId": show, "startId": uuid, "shuffle": false,
        }}})))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("CreatePlayQueue"), "application/json"),
        )
        .expect(1)
        .mount(&mock)
        .await;
    let queue = server.create_play_queue(&episode, None, false).await.unwrap();
    let index = queue.selected_index().expect("the queue says where to start");
    let playing = &queue.items[index];
    let (queue_id, entry) = playing
        .play_queue_item_id
        .unwrap()
        .as_str()
        .split_once(':')
        .unwrap();
    assert_eq!(queue_id, queue.id.as_str());

    let loaded = server.item(playing.rating_key).await.unwrap();
    // Video the stream cannot carry as it is (anything but H.264 and
    // HEVC) is converted, also when asked to play it as it is.
    let fits = loaded
        .streams(StreamKind::Video)
        .all(|stream| matches!(stream.codec.as_deref(), Some("h264" | "hevc")));
    let url = server.stream_url(&loaded, true).unwrap();
    assert!(url.contains(&format!(
        "/master.m3u8?direct={fits}&transcode={}&subtitleFormat=SRT&token=stream-token",
        !fits
    )));
    // A movie in HEVC plays as it is, or converted when asked.
    let movie = server.item("movie-any".into()).await.unwrap();
    assert!(server.stream_url(&movie, true).unwrap().contains("direct=true&transcode=false"));
    assert!(server.stream_url(&movie, false).unwrap().contains("direct=false&transcode=true"));

    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(header("authorization", "Bearer access-for-refresh-1"))
        .and(body_string_contains("UpdatePlayQueue"))
        .and(body_partial_json(json!({"variables": {
            "id": queue_id, "item": entry, "progress": 61000, "state": "PLAYING",
            "device": "6f1c2a0e-0000-4000-8000-00000000abcd",
            "settings": {"direct": fits, "transcode": !fits, "subtitleFormat": "SRT"},
        }})))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("UpdatePlayQueue"), "application/json"),
        )
        .expect(1)
        .mount(&mock)
        .await;
    server
        .update_timeline(playing, 61_000, PlaybackState::Playing, None)
        .await
        .unwrap();

    // An item that is in no queue of the server has nothing to report to.
    server
        .update_timeline(&loaded, 61_000, PlaybackState::Playing, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_search_is_asked_once_for_all_kinds() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let mut found = 0;
    for kind in [ItemKind::Movie, ItemKind::Show, ItemKind::Episode, ItemKind::Album] {
        let items = server.search("a", kind, 10).await.unwrap();
        assert!(items.iter().all(|item| item.kind == kind));
        found += items.len();
    }
    assert!(found > 0);
    let searches = mock
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == "/api/graphql" && operation(request) == "Search")
        .count();
    assert_eq!(searches, 1);
}

#[tokio::test]
async fn what_the_server_refuses_is_told_apart() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    let answer = |message: &str| {
        ResponseTemplate::new(200)
            .set_body_json(json!({"data": null, "errors": [{"message": message}]}))
    };
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("query Playlists"))
        .respond_with(answer("Unauthorized"))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("query Search"))
        .respond_with(answer("Search is not available"))
        .mount(&mock)
        .await;
    assert!(matches!(server.playlists().await, Err(Error::Unauthorized)));
    assert!(matches!(
        server.search("a", ItemKind::Movie, 5).await,
        Err(Error::Server(message)) if message == "Search is not available"
    ));
    // An id that is not one of this server is not asked for at all.
    assert!(matches!(server.item("42".into()).await, Err(Error::NotFound(_))));
}

#[tokio::test]
async fn subtitles_that_are_pictures_come_with_their_sheets() {
    let mock = serve(300).await;
    let (server, _) = connect(&mock).await;
    Mock::given(method("GET"))
        .and(path("/hls/file/bsub_stream.json"))
        .and(wiremock::matchers::query_param("token", "stream-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": 1, "width": 720, "height": 576,
            "sheets": ["bsub_stream_00.png", "bsub_stream_01.png"],
            "cues": [
                {"s": 7560, "e": 10438, "x": 132, "y": 85, "w": 457, "h": 28, "sheet": 1, "sx": 0, "sy": 67, "forced": false},
                {"s": 3440, "e": 7399, "x": 135, "y": 430, "w": 450, "h": 65, "sheet": 0, "sx": 0, "sy": 0, "forced": false},
            ],
        })))
        .mount(&mock)
        .await;
    let key = format!("{}/hls/file/bsub_stream.json", mock.uri());
    let subtitles = server.picture_subtitles(&key).await.unwrap();
    assert_eq!((subtitles.width, subtitles.height), (720.0, 576.0));
    // The sheets are beside the list, to fetch like any file of the server.
    assert_eq!(subtitles.sheets[1], format!("{}/hls/file/bsub_stream_01.png", mock.uri()));
    let first = subtitles.active_at(4_000);
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].y, first[0].sheet), (430.0, 0));
    assert!(subtitles.active_at(7_450).is_empty());
    let second = subtitles.active_at(8_000);
    assert_eq!((second[0].sheet, second[0].sheet_y), (1, 67.0));

    // A stream the server has no pictures for.
    let missing = format!("{}/hls/file/bsub_other.json", mock.uri());
    assert!(matches!(
        server.picture_subtitles(&missing).await,
        Err(Error::Status { .. })
    ));
}

#[tokio::test]
async fn a_provider_that_gives_no_lasting_session_is_asked_without() {
    let mock = serve(300).await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let well_known = auth::discover(&http, &mock.uri()).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    let pages = auth::Pages {
        signed_in: "Je bent <aangemeld>".into(),
        failed: "Mislukt".into(),
    };
    let login = Login::start(provider, false, pages).await.unwrap();
    let page = url::Url::parse(&login.url).unwrap();
    let asked: std::collections::HashMap<String, String> = page.query_pairs().into_owned().collect();
    // First with the wish to stay signed in.
    assert_eq!(asked["scope"], "openid offline_access");
    let redirect = asked["redirect_uri"].replace("localhost", "127.0.0.1");
    let state = asked["state"].clone();

    let browser = async {
        // The provider refuses the wish; the browser is sent to ask again.
        let refused = http
            .get(format!("{redirect}/?error=invalid_scope&state={state}"))
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status(), 302);
        let again = url::Url::parse(refused.headers()["location"].to_str().unwrap()).unwrap();
        let asked: std::collections::HashMap<String, String> =
            again.query_pairs().into_owned().collect();
        assert_eq!(asked["scope"], "openid");
        assert_eq!(asked["state"], state);
        // That works, and the page says so in the words it was given.
        let done = http
            .get(format!("{redirect}/?code=the-code&state={state}"))
            .send()
            .await
            .unwrap();
        assert!(done.text().await.unwrap().contains("Je bent &lt;aangemeld&gt;"));
    };
    let (tokens, ()) = tokio::join!(login.finish(&http), browser);
    assert_eq!(tokens.unwrap().refresh, "refresh-from-code");
}

#[tokio::test]
async fn a_provider_that_refuses_twice_has_another_reason() {
    let mock = serve(300).await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let well_known = auth::discover(&http, &mock.uri()).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    let login = Login::start(provider, false, Default::default()).await.unwrap();
    let redirect = url::Url::parse(&login.url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "redirect_uri")
        .unwrap()
        .1
        .replace("localhost", "127.0.0.1");
    let browser = async {
        for _ in 0..2 {
            let _ = http
                .get(format!("{redirect}/?error=invalid_scope&error_description=no"))
                .send()
                .await;
        }
    };
    let (tokens, ()) = tokio::join!(login.finish(&http), browser);
    assert!(matches!(tokens, Err(Error::Server(reason)) if reason == "no"));
}
