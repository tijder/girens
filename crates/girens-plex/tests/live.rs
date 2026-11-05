//! Read-only smoke test against a real server. Not run by default:
//!
//! ```sh
//! PLEX_URL=https://plex.example.org PLEX_TOKEN=... \
//!     cargo test -p girens-plex --test live -- --ignored --nocapture
//! ```

use girens_plex::model::ItemKind;
use girens_plex::{ClientInfo, PlexServer};

#[tokio::test]
#[ignore = "needs PLEX_URL and PLEX_TOKEN"]
async fn browse_a_real_server() {
    let url = std::env::var("PLEX_URL").expect("PLEX_URL");
    let token = std::env::var("PLEX_TOKEN").expect("PLEX_TOKEN");
    let client = ClientInfo {
        identifier: "girens-plex-live-test".into(),
        product: "Girens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        device_name: "live test".into(),
    };

    let server = PlexServer::connect(&url, &token, &client).await.unwrap();
    assert!(!server.machine_identifier().is_empty());

    let sections = server.sections().await.unwrap();
    println!("{} sections", sections.len());
    server.recently_added().await.unwrap();
    server.on_deck().await.unwrap();
    println!("{} playlists", server.playlists().await.unwrap().len());

    for section in &sections {
        if !matches!(
            section.kind,
            ItemKind::Movie | ItemKind::Show | ItemKind::Artist
        ) {
            continue;
        }
        server.section_sorts(section.key).await.unwrap();
        let page = server
            .section_items(section.key, 0, 50, Some("titleSort:asc"))
            .await
            .unwrap();
        println!(
            "section {:?}: {} of {} items",
            section.kind,
            page.items.len(),
            page.total
        );
        let Some(first) = page.items.first() else {
            continue;
        };
        let item = server.item(first.rating_key).await.unwrap();
        assert_eq!(item.rating_key, first.rating_key);
        if item.kind != ItemKind::Movie {
            let children = server.children(item.rating_key).await.unwrap();
            println!("  first item has {} children", children.len());
        }
        if let Some(thumb) = item.cover() {
            let image = reqwest::get(server.image_url(thumb, 300, 200))
                .await
                .unwrap();
            assert!(image.status().is_success(), "image: {}", image.status());
        }
    }
}

/// Asks plex.tv for a sign-in pin, as the "sign in in the browser" flow
/// does. Nobody approves it, so it stays pending and expires by itself.
#[tokio::test]
#[ignore = "talks to plex.tv"]
async fn a_browser_sign_in_can_be_started() {
    use girens_plex::myplex::MyPlex;

    let client = ClientInfo {
        identifier: "girens-plex-live-test".into(),
        product: "Girens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        device_name: "live test".into(),
    };
    let pin = MyPlex::create_pin(&client).await.unwrap();
    assert!(pin.id > 0);
    assert!(pin.code.len() > 4, "strong pins are long");
    assert!(pin.auth_token.is_none());
    assert!(pin
        .auth_url(&client)
        .starts_with("https://app.plex.tv/auth#?clientID="));
    assert!(MyPlex::check_pin(&pin, &client).await.unwrap().is_none());
}

/// Interrupts a real download and continues it, as the offline sync does.
#[tokio::test]
#[ignore = "needs PLEX_URL and PLEX_TOKEN"]
async fn a_real_download_can_be_continued() {
    let url = std::env::var("PLEX_URL").expect("PLEX_URL");
    let token = std::env::var("PLEX_TOKEN").expect("PLEX_TOKEN");
    let client = ClientInfo {
        identifier: "girens-plex-live-test".into(),
        product: "Girens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        device_name: "live test".into(),
    };
    let server = PlexServer::connect(&url, &token, &client).await.unwrap();

    let dir = std::env::temp_dir().join(format!("girens-plex-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let whole = dir.join("whole");
    let resumed = dir.join("resumed");

    // A music track: small enough to download twice. Libraries can list
    // tracks whose file is gone, so take the first that downloads.
    let mut found = None;
    'search: for section in server.sections().await.unwrap() {
        if section.kind != ItemKind::Artist {
            continue;
        }
        for album in server
            .section_albums(section.key)
            .await
            .unwrap()
            .iter()
            .take(5)
        {
            for track in server
                .children(album.rating_key)
                .await
                .unwrap()
                .iter()
                .take(2)
            {
                let Some(part) = track.parts().next() else {
                    continue;
                };
                let download = server.download_url(&part.key);
                if girens_plex::download_to_file(download.as_str(), &whole)
                    .await
                    .is_ok()
                {
                    found = Some((download, part.size.expect("the size of the file")));
                    break 'search;
                }
            }
        }
    }
    let (download, size) = found.expect("a track that can be downloaded");
    let complete = std::fs::read(&whole).unwrap();
    assert_eq!(complete.len() as u64, size);

    // What an interrupted download leaves behind: the first third.
    let cut = complete.len() / 3;
    std::fs::write(dir.join("resumed.tmp"), &complete[..cut]).unwrap();
    let first_report = std::sync::Arc::new(std::sync::Mutex::new(None));
    let seen = first_report.clone();
    girens_plex::download_to_file_with_progress(
        download.as_str(),
        &resumed,
        true,
        move |received, _| {
            seen.lock().unwrap().get_or_insert(received);
        },
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&resumed).unwrap(), complete);
    let first = first_report.lock().unwrap().expect("progress was reported");
    assert!(
        first > cut as u64,
        "the download started over instead of continuing"
    );
    println!("continued a {size} byte file from byte {cut}");
    std::fs::remove_dir_all(&dir).unwrap();
}
