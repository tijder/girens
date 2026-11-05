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
