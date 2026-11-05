//! Smoke test of the calls that change state on a real server. Everything it
//! changes is put back: it uses one unwatched, unrated movie for the watched
//! state, rating and timeline, and one multi-audio item for stream selection.
//! Not run by default:
//!
//! ```sh
//! PLEX_URL=... PLEX_TOKEN=... [PLEX_ACCOUNT_TOKEN=...] \
//!     cargo test -p girens-plex --test live_write -- --ignored --nocapture
//! ```

use girens_plex::model::{Item, ItemKind, StreamKind};
use girens_plex::myplex::MyPlex;
use girens_plex::playback::{PlaybackState, TranscodeRequest};
use girens_plex::{ClientInfo, PlexServer};

fn client() -> ClientInfo {
    ClientInfo {
        identifier: "girens-plex-live-test".into(),
        product: "Girens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        device_name: "live test".into(),
    }
}

async fn server() -> PlexServer {
    let url = std::env::var("PLEX_URL").expect("PLEX_URL");
    let token = std::env::var("PLEX_TOKEN").expect("PLEX_TOKEN");
    PlexServer::connect(&url, &token, &client()).await.unwrap()
}

async fn movies(server: &PlexServer) -> Vec<Item> {
    let sections = server.sections().await.unwrap();
    let section = sections
        .iter()
        .find(|section| section.kind == ItemKind::Movie)
        .expect("a movie section");
    server
        .section_items(section.key, 0, 60, Some("addedAt:asc"))
        .await
        .unwrap()
        .items
}

#[tokio::test]
#[ignore = "needs PLEX_ACCOUNT_TOKEN"]
async fn plex_tv_account_and_servers() {
    let token = std::env::var("PLEX_ACCOUNT_TOKEN").expect("PLEX_ACCOUNT_TOKEN");
    let myplex = MyPlex::with_token(&token, &client()).await.unwrap();
    let account = myplex.account();
    assert!(!account.uuid.is_empty());
    println!(
        "account ok, subscription active: {:?}",
        account.subscription.as_ref().map(|s| s.active)
    );

    let servers = myplex.servers().await.unwrap();
    println!("{} server(s)", servers.len());
    assert!(!servers.is_empty());
    for resource in &servers {
        println!(
            "  owned={} connections={}",
            resource.owned,
            resource.connections.len()
        );
    }
    let owned = servers.iter().find(|s| s.owned).unwrap_or(&servers[0]);
    let server = myplex.connect(owned).await.unwrap();
    assert_eq!(server.machine_identifier(), owned.client_identifier);
    println!("connected through {}", server.base_url().scheme());
}

#[tokio::test]
#[ignore = "needs PLEX_URL and PLEX_TOKEN"]
async fn watched_state_rating_and_timeline_round_trip() {
    let server = server().await;
    let movie = movies(&server)
        .await
        .into_iter()
        .find(|m| m.view_count == 0 && m.view_offset == 0 && m.user_rating.is_none())
        .expect("an unwatched, unrated movie");
    let key = movie.rating_key;

    server.mark_watched(key).await.unwrap();
    assert!(server.item(key).await.unwrap().is_watched());
    server.mark_unwatched(key).await.unwrap();
    assert!(!server.item(key).await.unwrap().is_watched());
    println!("watched state ok");

    server.rate(key, 8.0).await.unwrap();
    assert_eq!(server.item(key).await.unwrap().user_rating, Some(8.0));
    server.rate(key, -1.0).await.unwrap();
    let rating = server.item(key).await.unwrap().user_rating;
    assert!(rating.is_none(), "rating was not cleared: {rating:?}");
    println!("rating ok");

    let item = server.item(key).await.unwrap();
    let position = 120_000;
    server
        .update_timeline(
            &item,
            position,
            PlaybackState::Playing,
            Some("girens-live-test"),
        )
        .await
        .unwrap();
    server
        .update_timeline(
            &item,
            position,
            PlaybackState::Stopped,
            Some("girens-live-test"),
        )
        .await
        .unwrap();
    let offset = server.item(key).await.unwrap().view_offset;
    println!("timeline ok, server stored view offset {offset} ms");
    assert_eq!(offset, position);

    // Put the movie back to "never played".
    server.mark_unwatched(key).await.unwrap();
    let restored = server.item(key).await.unwrap();
    assert_eq!((restored.view_count, restored.view_offset), (0, 0));
    println!("restored");
}

#[tokio::test]
#[ignore = "needs PLEX_URL and PLEX_TOKEN"]
async fn play_queue_and_transcode_decision() {
    let server = server().await;
    let movie = movies(&server).await.into_iter().next().expect("a movie");

    let queue = server.create_play_queue(&movie, None, false).await.unwrap();
    println!(
        "queue with {} item(s), shuffled={}",
        queue.items.len(),
        queue.shuffled
    );
    assert!(!queue.items.is_empty());
    assert!(queue.selected_index().is_some());
    assert!(queue
        .items
        .iter()
        .all(|item| item.play_queue_item_id.is_some()));

    let refreshed = server.refresh_play_queue(queue.id).await.unwrap();
    assert_eq!(refreshed.id, queue.id);
    let shuffled = server.shuffle_play_queue(queue.id, true).await.unwrap();
    assert!(shuffled.shuffled);
    let unshuffled = server.shuffle_play_queue(queue.id, false).await.unwrap();
    assert!(!unshuffled.shuffled);
    println!("play queue ok");

    let sections = server.sections().await.unwrap();
    let section = sections.iter().find(|s| s.kind == ItemKind::Movie).unwrap();
    let section_queue = server
        .create_section_play_queue(section, Some("titleSort:asc"), true)
        .await
        .unwrap();
    assert!(!section_queue.items.is_empty());
    println!(
        "section queue ok ({} items in window)",
        section_queue.items.len()
    );

    let request = TranscodeRequest {
        session: "girens-live-test".into(),
        video_resolution: Some("1280x720".into()),
        offset: 0,
    };
    server.transcode_decision(&movie, &request).await.unwrap();
    println!("transcode decision ok");
}

#[tokio::test]
#[ignore = "needs PLEX_URL and PLEX_TOKEN"]
async fn stream_selection_round_trip() {
    let server = server().await;
    let mut found = None;
    for movie in movies(&server).await.into_iter().take(40) {
        let item = server.item(movie.rating_key).await.unwrap();
        if item.streams(StreamKind::Audio).count() >= 2
            && item.selected_stream(StreamKind::Audio).is_some()
        {
            found = Some(item);
            break;
        }
    }
    let Some(item) = found else {
        println!("no movie with two audio streams among the first 40; skipped");
        return;
    };
    let part = item.parts().next().unwrap().clone();
    let original_audio = item.selected_stream(StreamKind::Audio).unwrap().id;
    let original_subtitle = item.selected_stream(StreamKind::Subtitle).map(|s| s.id);
    let other_audio = item
        .streams(StreamKind::Audio)
        .find(|stream| stream.id != original_audio)
        .unwrap()
        .id;

    server
        .select_audio_stream(&part, other_audio)
        .await
        .unwrap();
    let changed = server.item(item.rating_key).await.unwrap();
    assert_eq!(
        changed.selected_stream(StreamKind::Audio).unwrap().id,
        other_audio
    );
    server
        .select_audio_stream(&part, original_audio)
        .await
        .unwrap();
    println!("audio selection ok");

    if let Some(subtitle) = item.streams(StreamKind::Subtitle).next().map(|s| s.id) {
        server
            .select_subtitle_stream(&part, Some(subtitle))
            .await
            .unwrap();
        let changed = server.item(item.rating_key).await.unwrap();
        assert_eq!(
            changed.selected_stream(StreamKind::Subtitle).map(|s| s.id),
            Some(subtitle)
        );
        server.select_subtitle_stream(&part, None).await.unwrap();
        let cleared = server.item(item.rating_key).await.unwrap();
        assert!(cleared.selected_stream(StreamKind::Subtitle).is_none());
        println!("subtitle selection ok");
    }
    server
        .select_subtitle_stream(&part, original_subtitle)
        .await
        .unwrap();

    let restored = server.item(item.rating_key).await.unwrap();
    assert_eq!(
        restored.selected_stream(StreamKind::Audio).unwrap().id,
        original_audio
    );
    assert_eq!(
        restored.selected_stream(StreamKind::Subtitle).map(|s| s.id),
        original_subtitle
    );
    println!("restored");
}
