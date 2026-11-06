//! Tests against a real server; run them with
//! `ISTER_URL=localhost:8080 ISTER_USER=ister ISTER_PASSWORD=ister cargo test -p girens-ister -- --ignored`.
//!
//! The password is traded for tokens directly, which only a development
//! provider allows; Girens itself signs in through the browser.

use std::sync::Arc;

use girens_ister::auth::{self, Provider};
use girens_ister::IsterServer;
use girens_model::model::ItemKind;
use girens_model::PlaybackState;

async fn connect() -> Option<IsterServer> {
    let (Ok(address), Ok(user), Ok(password)) = (
        std::env::var("ISTER_URL"),
        std::env::var("ISTER_USER"),
        std::env::var("ISTER_PASSWORD"),
    ) else {
        return None;
    };
    let http = reqwest::Client::new();
    let well_known = auth::discover(&http, &address).await.unwrap();
    let provider = Provider::fetch(&http, &well_known.openid_url).await.unwrap();
    let form = [
        ("grant_type", "password"),
        ("client_id", "ister"),
        ("username", user.as_str()),
        ("password", password.as_str()),
        ("scope", "openid"),
    ];
    let answer: serde_json::Value = http
        .post(&provider.token_endpoint)
        .form(&form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let refresh = answer["refresh_token"].as_str().unwrap();
    let tokens = auth::refresh(&http, &provider, refresh).await.unwrap();
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
        Arc::new(|_| {}),
    )
    .await
    .unwrap();
    // `ISTER_RECORD=<dir>` renews the test data of `tests/server.rs`.
    if let Ok(dir) = std::env::var("ISTER_RECORD") {
        server.record_into(std::path::Path::new(&dir));
    }
    Some(server)
}

#[tokio::test]
#[ignore = "needs ISTER_URL, ISTER_USER and ISTER_PASSWORD"]
async fn browse_a_real_server() {
    let Some(server) = connect().await else {
        return;
    };
    println!("{} as {:?}", server.machine_identifier(), server.user());
    let sections = server.sections().await.unwrap();
    assert!(!sections.is_empty());
    for section in &sections {
        let page = server.section_items(section.key, 0, 50, None).await.unwrap();
        println!("{} ({:?}): {} of {}", section.title, section.kind, page.items.len(), page.total);
        let Some(first) = page.items.first() else {
            continue;
        };
        let item = server.item(first.rating_key).await.unwrap();
        assert_eq!(item.title, first.title);
        match section.kind {
            ItemKind::Show => {
                let seasons = server.children(item.rating_key).await.unwrap();
                let episodes = server.children(seasons[0].rating_key).await.unwrap();
                let all = server.all_leaves(item.rating_key).await.unwrap();
                println!("  {}: {} seasons, {} episodes in the first, {} in all; watched {:?}/{:?}",
                    item.title, seasons.len(), episodes.len(), all.len(), item.viewed_leaf_count, item.leaf_count);
                let episode = server.item(episodes[0].rating_key).await.unwrap();
                println!("  {} {:?} {:?} thumb {:?}", episode.title, episode.season_episode(), episode.duration, episode.thumb);
                let season = server.item(seasons[0].rating_key).await.unwrap();
                assert_eq!(season.kind, ItemKind::Season);
            }
            // Books and podcasts are looked at below.
            ItemKind::Artist if !section.key.as_str().starts_with("artist-") => {}
            ItemKind::Artist => {
                let albums = server.children(item.rating_key).await.unwrap();
                let tracks = server.children(albums[0].rating_key).await.unwrap();
                let all = server.all_leaves(item.rating_key).await.unwrap();
                println!("  {}: {} albums, {} tracks on the first, {} in all", item.title, albums.len(), tracks.len(), all.len());
                let every_album = server.section_albums(section.key).await.unwrap();
                assert!(every_album.len() >= albums.len());
                let track = server.item(tracks[0].rating_key).await.unwrap();
                println!("  {} by {:?} on {:?}: {}", track.title, track.grandparent_title, track.parent_title,
                    server.stream_url(&track, true).unwrap().split("token=").next().unwrap());
            }
            _ => {
                let part = item.parts().next().unwrap();
                println!("  {} ({:?}): {} streams, cover {:?}", item.title, item.year, part.streams.len(),
                    item.thumb.as_deref().map(|thumb| server.image_url(thumb, 200, 300).split("token=").next().unwrap().to_owned()));
            }
        }
    }
    // A book to listen to, when the server has one.
    if let Some(books) = sections.iter().find(|section| section.key.as_str().starts_with("book-")) {
        let listed = server.section_items(books.key, 0, 50, None).await.unwrap();
        if let Some(book) = listed.items.first() {
            let book = server.item(book.rating_key).await.unwrap();
            let chapters = server.children(book.rating_key).await.unwrap();
            let chapter = server.item(chapters[0].rating_key).await.unwrap();
            println!("  {} by {:?}: {} chapters, first {:?} ({:?} ms)", book.title, book.parent_title,
                chapters.len(), chapter.title, chapter.duration);
            let queue = server.create_play_queue(&book, Some(&chapters[1]), false).await.unwrap();
            let index = queue.selected_index().unwrap();
            assert_eq!(queue.items[index].rating_key, chapters[1].rating_key);
            let master = reqwest::get(server.stream_url(&chapter, true).unwrap()).await.unwrap();
            assert!(master.status().is_success());
        }
    }
    // A podcast, when the server has one with episodes it fetched.
    if let Some(podcasts) = sections.iter().find(|section| section.key.as_str().starts_with("podcast-")) {
        let listed = server.section_items(podcasts.key, 0, 50, None).await.unwrap();
        if let Some(podcast) = listed.items.first() {
            let podcast = server.item(podcast.rating_key).await.unwrap();
            let episodes = server.children(podcast.rating_key).await.unwrap();
            println!("  {}: {} episodes, first {:?}", podcast.title, episodes.len(),
                episodes.first().map(|episode| &episode.title));
            if let Some(first) = episodes.first() {
                assert_eq!(Some(episodes.len() as u32), podcast.leaf_count);
                let episode = server.item(first.rating_key).await.unwrap();
                assert_eq!(episode.kind, ItemKind::Track);
                let queue = server.create_play_queue(&podcast, Some(first), false).await.unwrap();
                let index = queue.selected_index().unwrap();
                assert_eq!(queue.items[index].rating_key, first.rating_key);
                let master = reqwest::get(server.stream_url(&episode, true).unwrap()).await.unwrap();
                assert!(master.status().is_success(), "{}", master.status());
            }
        }
    }
    println!("recently added: {}", server.recently_added().await.unwrap().len());
    println!("on deck: {}", server.on_deck().await.unwrap().len());
    println!("playlists: {}", server.playlists().await.unwrap().len());
    for kind in [ItemKind::Movie, ItemKind::Show, ItemKind::Album, ItemKind::Track] {
        println!("search {kind:?}: {}", server.search("a", kind, 10).await.unwrap().len());
    }
}

#[tokio::test]
#[ignore = "needs ISTER_URL, ISTER_USER and ISTER_PASSWORD; changes what was watched"]
async fn play_and_mark_on_a_real_server() {
    let Some(server) = connect().await else {
        return;
    };
    let sections = server.sections().await.unwrap();
    let movies = sections.iter().find(|section| section.kind == ItemKind::Movie).unwrap();
    let movie = server.section_items(movies.key, 0, 5, None).await.unwrap().items.remove(0);

    let queue = server.create_play_queue(&movie, None, false).await.unwrap();
    let playing = &queue.items[queue.selected_index().unwrap_or(0)];
    assert_eq!(playing.rating_key, movie.rating_key);
    let loaded = server.item(movie.rating_key).await.unwrap();
    let url = server.stream_url(&loaded, true).unwrap();
    let master = reqwest::get(&url).await.unwrap();
    assert!(master.status().is_success(), "{}", master.status());
    assert!(master.text().await.unwrap().starts_with("#EXTM3U"));

    server.mark_watched(movie.rating_key).await.unwrap();
    assert!(server.item(movie.rating_key).await.unwrap().is_watched());
    server.mark_unwatched(movie.rating_key).await.unwrap();
    let cleared = server.item(movie.rating_key).await.unwrap();
    assert!(!cleared.is_watched());
    assert_eq!(cleared.view_offset, 0);

    server.rate(movie.rating_key, 8.0).await.unwrap();
    assert_eq!(server.item(movie.rating_key).await.unwrap().user_rating, Some(8.0));
    server.rate(movie.rating_key, 0.0).await.unwrap();
    assert_eq!(server.item(movie.rating_key).await.unwrap().user_rating, None);

    // A show plays from the chosen episode on, and can be reshuffled.
    let shows = sections.iter().find(|section| section.kind == ItemKind::Show).unwrap();
    let show = server.section_items(shows.key, 0, 5, None).await.unwrap().items.remove(0);
    let episodes = server.all_leaves(show.rating_key).await.unwrap();
    let queue = server.create_play_queue(&episodes[3], None, false).await.unwrap();
    let index = queue.selected_index().unwrap();
    assert_eq!(queue.items[index].rating_key, episodes[3].rating_key);
    assert_eq!(queue.items[index + 1].rating_key, episodes[4].rating_key);
    let again = server.refresh_play_queue(queue.id).await.unwrap();
    assert_eq!(again.items.len(), queue.items.len());
    let shuffled = server.shuffle_play_queue(queue.id, true).await.unwrap();
    assert!(shuffled.shuffled);

    // A whole show is marked episode by episode.
    server.mark_watched(show.rating_key).await.unwrap();
    let watched = server.item(show.rating_key).await.unwrap();
    assert_eq!(watched.viewed_leaf_count, watched.leaf_count);
    assert!(watched.is_watched());
    server.mark_unwatched(show.rating_key).await.unwrap();
    assert_eq!(server.item(show.rating_key).await.unwrap().viewed_leaf_count, Some(0));

    // The server keeps a position from a minute in until a minute before
    // the end, which only the episodes of the test data are long enough for.
    let playing = &queue.items[index];
    server.update_timeline(playing, 70_000, PlaybackState::Playing, None).await.unwrap();
    server.update_timeline(playing, 75_000, PlaybackState::Stopped, None).await.unwrap();
    let resumed = server.item(playing.rating_key).await.unwrap();
    assert_eq!(resumed.view_offset, 75_000);
    assert!(server.on_deck().await.unwrap().iter().any(|item| item.rating_key == playing.rating_key));
    server.mark_unwatched(playing.rating_key).await.unwrap();
    assert_eq!(server.item(playing.rating_key).await.unwrap().view_offset, 0);
}
