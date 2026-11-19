//! plex.tv calls against a mock that answers the way plex.tv does. The
//! responses have the fields of real ones (recorded on 2026-11-06) with
//! made-up values.

use std::time::{Duration, Instant};

use girens_plex::myplex::{Connection, MyPlex, Resource};
use girens_plex::{ClientInfo, Error};
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str, status: u16) -> ResponseTemplate {
    let file = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let body = std::fs::read_to_string(&file).unwrap_or_else(|err| panic!("{file}: {err}"));
    ResponseTemplate::new(status).set_body_raw(body, "application/json")
}

fn client_info() -> ClientInfo {
    ClientInfo {
        identifier: "test-client".into(),
        product: "Girens".into(),
        version: "0".into(),
        device_name: "test".into(),
    }
}

/// A plex.tv with a Home of three: the admin who is signed in, a member
/// with a PIN and a managed profile.
async fn sign_in() -> (MockServer, MyPlex) {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/user"))
        .and(header("x-plex-token", "account-token"))
        .respond_with(fixture("plextv_user", 200))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/home/users"))
        .and(header("x-plex-token", "account-token"))
        .respond_with(fixture("plextv_home_users", 200))
        .mount(&mock)
        .await;
    let myplex = MyPlex::with_token_at(&mock.uri(), "account-token", &client_info())
        .await
        .expect("sign in");
    (mock, myplex)
}

#[tokio::test]
async fn the_profiles_of_the_home_are_listed() {
    let (_mock, myplex) = sign_in().await;
    let users = myplex.home_users().await.unwrap();
    assert_eq!(users.len(), 3);
    assert_eq!(users[0].uuid, myplex.account().uuid);
    assert!(users[0].admin && !users[0].protected);
    assert!(users[1].protected && !users[1].restricted);
    assert!(users[2].restricted && !users[2].protected);
    assert_eq!(users[2].title, "title-3");
}

#[tokio::test]
async fn switching_gives_a_session_as_the_other_profile() {
    let (mock, myplex) = sign_in().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/home/users/uuid-3/switch"))
        .and(header("x-plex-token", "account-token"))
        .and(header("x-plex-client-identifier", "test-client"))
        .and(query_param_is_missing("pin"))
        .respond_with(fixture("plextv_switch", 201))
        .mount(&mock)
        .await;
    // What the profile may use is asked with its own token.
    Mock::given(method("GET"))
        .and(path("/api/v2/resources"))
        .and(header("x-plex-token", "profile-token"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("[]", "application/json"))
        .mount(&mock)
        .await;

    let users = myplex.home_users().await.unwrap();
    let profile = myplex.switch_user(&users[2].uuid, None).await.unwrap();
    assert_eq!(profile.account().uuid, "uuid-3");
    assert_eq!(profile.account().auth_token, "profile-token");
    assert!(profile.account().username.is_none());
    assert_eq!(profile.account().title.as_deref(), Some("title-3"));
    assert!(profile.servers().await.unwrap().is_empty());
    // The account that switched is still signed in.
    assert_eq!(myplex.account().auth_token, "account-token");
}

#[tokio::test]
async fn a_protected_profile_takes_its_pin() {
    let (mock, myplex) = sign_in().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/home/users/uuid-2/switch"))
        .and(query_param("pin", "1234"))
        .respond_with(fixture("plextv_switch", 201))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/home/users/uuid-2/switch"))
        .respond_with(ResponseTemplate::new(403).set_body_raw(
            r#"{"errors":[{"code":1041,"message":"Invalid PIN","status":403}]}"#,
            "application/json",
        ))
        .with_priority(10)
        .mount(&mock)
        .await;

    let users = myplex.home_users().await.unwrap();
    let wrong = myplex.switch_user(&users[1].uuid, Some("0000")).await;
    assert!(matches!(wrong, Err(Error::WrongPin)), "{wrong:?}");
    let missing = myplex.switch_user(&users[1].uuid, None).await;
    assert!(matches!(missing, Err(Error::WrongPin)), "{missing:?}");
    assert!(myplex
        .switch_user(&users[1].uuid, Some("1234"))
        .await
        .is_ok());
}

#[tokio::test]
async fn a_profile_that_is_gone_is_not_a_wrong_pin() {
    let (mock, myplex) = sign_in().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/home/users/uuid-2/switch"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;
    let users = myplex.home_users().await.unwrap();
    let gone = myplex.switch_user(&users[1].uuid, Some("1234")).await;
    match gone {
        Err(Error::Status { status, url }) => {
            assert_eq!(status.as_u16(), 404);
            assert!(!url.contains("1234"), "the PIN is not logged");
        }
        other => panic!("{other:?}"),
    }
}

/// A media server that answers after `delay`.
async fn media_server(delay: Duration) -> MockServer {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(fixture("server_root", 200).set_delay(delay))
        .mount(&mock)
        .await;
    mock
}

/// A server of the account with a local and a remote address.
fn resource(local: &str, remote: &str) -> Resource {
    let connection = |uri: &str, local| Connection {
        uri: uri.to_owned(),
        local,
        relay: false,
    };
    Resource {
        name: "Server".into(),
        client_identifier: "machine".into(),
        provides: "server".into(),
        owned: true,
        access_token: Some("server-token".into()),
        connections: vec![connection(remote, false), connection(local, true)],
    }
}

#[tokio::test]
async fn an_address_that_does_not_answer_does_not_hold_up_the_others() {
    let (_plex_tv, myplex) = sign_in().await;
    let local = media_server(Duration::from_secs(30)).await;
    let remote = media_server(Duration::ZERO).await;

    let started = Instant::now();
    let server = myplex
        .connect(&resource(&local.uri(), &remote.uri()))
        .await
        .expect("connect");
    assert_eq!(server.base_url().as_str(), format!("{}/", remote.uri()));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn the_preferred_address_wins_when_it_answers_a_little_later() {
    let (_plex_tv, myplex) = sign_in().await;
    let local = media_server(Duration::from_millis(300)).await;
    let remote = media_server(Duration::ZERO).await;

    let server = myplex
        .connect(&resource(&local.uri(), &remote.uri()))
        .await
        .expect("connect");
    assert_eq!(server.base_url().as_str(), format!("{}/", local.uri()));
}

#[tokio::test]
async fn no_address_that_answers_is_no_connection() {
    let (_plex_tv, myplex) = sign_in().await;
    // Nothing listens on these ports.
    let gone = myplex
        .connect(&resource("http://127.0.0.1:1", "http://127.0.0.1:2"))
        .await;
    assert!(matches!(gone, Err(Error::NoConnection(_))), "{gone:?}");
}
