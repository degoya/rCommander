//! The secrets kind (RC-100-33): a source answers its state, entries and one value; what it
//! answers is held to the host's rules; it reaches the loopback port it was granted and nothing
//! else; a crash or a hang fails the one call.

use rc_plugin_api::secrets::{SecretField, SourceState};
use rc_plugin_types::FailureKind;
use secrecy::ExposeSecret as _;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
};

use super::{SecretsOptions, SecretsPlugin};
use crate::{
    failure::codes,
    manifest::parse_manifest,
    test_fixtures::{secrets_manifest, secrets_manifest_text},
    testbed::{
        self, SPIN, TRAP,
        secrets::{CANARY, EntryData, Export, LOCKED, SecretsBed, entry_bytes, get_over_http},
    },
};

fn plugin(loopback: Option<u16>, bed: &SecretsBed) -> SecretsPlugin {
    SecretsPlugin::new(
        &secrets_manifest(loopback),
        &bed.build(),
        SecretsOptions::default(),
    )
    .expect("a secret source")
}

fn code<T>(result: Result<T, rc_plugin_types::Failure>) -> String {
    result
        .err()
        .and_then(|failure| failure.code().map(str::to_owned))
        .expect("a coded failure")
}

#[tokio::test]
async fn a_source_answers_its_state_and_its_value() {
    let source = plugin(None, &SecretsBed::new());
    assert_eq!(source.status().await.expect("status"), SourceState::Ready);
    assert!(source.search("srv", 10).await.expect("search").is_empty());
    let value = source
        .get("7c17", SecretField::Password)
        .await
        .expect("get");
    assert_eq!(value.expose_secret(), CANARY);

    let locked = plugin(
        None,
        &SecretsBed::new().body(Export::Status, format!("(i32.const {LOCKED})")),
    );
    assert_eq!(locked.status().await.expect("status"), SourceState::Locked);
}

/// The value reaches the caller and nothing else: not the execution log, not its debug form.
#[tokio::test]
async fn the_value_stays_out_of_the_records() {
    let source = plugin(None, &SecretsBed::new());
    let value = source
        .get("7c17", SecretField::Password)
        .await
        .expect("get");
    assert!(!format!("{value:?}").contains(CANARY));
    let records = format!(
        "{:?}",
        source.execution_log().records(&source.identity().id)
    );
    assert!(records.contains("get"), "{records}");
    assert!(!records.contains(CANARY), "{records}");
}

/// Entries come back with ids that go back exactly and labels made safe to show.
#[tokio::test]
async fn entries_are_held_to_the_rules() {
    let mut bed = SecretsBed::new();
    let id = bed.data(b"7c17");
    let title = bed.data("srv01 \u{202e}root".as_bytes());
    let user = bed.data(b"root");
    let list = entry_bytes(&[EntryData {
        id,
        title,
        user: Some(user),
        url: None,
    }]);
    let entries = bed.data(&list);
    let source = plugin(
        None,
        &bed.body(Export::Search, testbed::ok_pair((entries.0, 1))),
    );
    let found = source.search("srv", 10).await.expect("search");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "7c17");
    assert!(!found[0].title.contains('\u{202e}'), "{}", found[0].title);
    assert_eq!(found[0].user.as_deref(), Some("root"));
    assert_eq!(found[0].url, None);

    // More entries than were asked for, and an empty id, are refused whole.
    assert_eq!(
        code(source.search("srv", 0).await),
        codes::PLUGIN_BAD_OUTPUT
    );
    let mut bed = SecretsBed::new();
    let title = bed.data(b"no id");
    let list = entry_bytes(&[EntryData {
        id: (title.0, 0),
        title,
        user: None,
        url: None,
    }]);
    let entries = bed.data(&list);
    let source = plugin(
        None,
        &bed.body(Export::Search, testbed::ok_pair((entries.0, 1))),
    );
    assert_eq!(
        code(source.search("srv", 10).await),
        codes::PLUGIN_BAD_OUTPUT
    );
}

#[tokio::test]
async fn an_empty_or_huge_value_and_a_strange_id_are_refused() {
    let empty = plugin(
        None,
        &SecretsBed::new().body(Export::Get, testbed::ok_pair((0, 0))),
    );
    assert_eq!(
        code(empty.get("7c17", SecretField::Password).await),
        codes::PLUGIN_BAD_OUTPUT
    );
    // Two more pages, so a string of 70 KiB lies in memory and is lifted before it is refused.
    let huge = SecretsBed::new().body(
        Export::Get,
        format!(
            "(drop (memory.grow (i32.const 2)))\n{}",
            testbed::ok_pair((0, 70 * 1024))
        ),
    );
    assert_eq!(
        code(plugin(None, &huge).get("7c17", SecretField::Password).await),
        codes::PLUGIN_OUTPUT_TOO_LARGE
    );
    let source = plugin(None, &SecretsBed::new());
    assert_eq!(
        code(source.get("", SecretField::Password).await),
        codes::PLUGIN_REQUEST_TOO_LARGE
    );
    assert_eq!(
        code(source.get("a\nb", SecretField::Password).await),
        codes::PLUGIN_REQUEST_TOO_LARGE
    );
}

#[tokio::test]
async fn a_crash_or_a_hang_fails_the_one_call() {
    let crashing = plugin(None, &SecretsBed::new().body(Export::Get, TRAP));
    assert_eq!(
        code(crashing.get("7c17", SecretField::Password).await),
        codes::PLUGIN_CRASHED
    );
    assert_eq!(crashing.status().await.expect("status"), SourceState::Ready);
    let hanging = plugin(None, &SecretsBed::new().body(Export::Search, SPIN));
    let started = std::time::Instant::now();
    // Spinning ends at the deadline or at the fuel, whichever comes first.
    let stopped = code(hanging.search("srv", 10).await);
    assert!(
        [codes::PLUGIN_TIMED_OUT, codes::PLUGIN_OUT_OF_FUEL].contains(&stopped.as_str()),
        "{stopped}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

/// A server on a free loopback port answering every connection with `body`, and the port.
async fn serve(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).await;
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(answer.as_bytes()).await;
        }
    });
    port
}

/// A source reading its value over `http` from `url`.
fn reading(url: &str) -> SecretsBed {
    let mut bed = SecretsBed::new();
    let method = bed.data(b"GET");
    let url = bed.data(url.as_bytes());
    bed.body(Export::Get, get_over_http(method, url))
}

/// The granted loopback port is reached; another port, and a granted port the host refuses
/// (rCommander's MCP port), are not. The bed hands the host's failure on as its own, and a
/// plugin's own failure keeps its kind but never a host code, so the denial shows as its kind.
#[tokio::test]
async fn a_source_reaches_its_granted_port_only() {
    let port = serve("from-the-bridge").await;
    let url = format!("http://localhost:{port}/object/password/7c17");
    let source = plugin(Some(port), &reading(&url));
    let value = source
        .get("7c17", SecretField::Password)
        .await
        .expect("get");
    assert_eq!(value.expose_secret(), "from-the-bridge");

    let other = plugin(Some(port.wrapping_add(1).max(1)), &reading(&url));
    let denied = other.get("7c17", SecretField::Password).await;
    assert_eq!(denied.expect_err("denied").kind, FailureKind::AccessDenied);

    let refused = SecretsPlugin::new(
        &secrets_manifest(Some(port)),
        &reading(&url).build(),
        SecretsOptions {
            refused_ports: vec![port],
            ..SecretsOptions::default()
        },
    )
    .expect("a secret source");
    let denied = refused.get("7c17", SecretField::Password).await;
    assert_eq!(denied.expect_err("denied").kind, FailureKind::AccessDenied);
}

/// A component importing `http` without `net_http` is refused before it runs.
#[test]
fn http_without_the_grant_is_refused() {
    let component = reading("http://localhost:8087/").build();
    let refused = SecretsPlugin::new(
        &secrets_manifest(None),
        &component,
        SecretsOptions::default(),
    );
    let failure = refused.err().expect("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_DENIED));
    assert_eq!(
        failure.params.get("import").map(String::as_str),
        Some(format!("rcommander:plugin/http@{}", rc_plugin_api::API_VERSION).as_str())
    );
    assert_eq!(failure.kind, FailureKind::AccessDenied);
    // Packaging refuses it too, where its author sees it; granted, it packages.
    let ungranted = secrets_manifest_text(None, "");
    assert!(crate::package_plugin(ungranted.as_bytes(), &component, &[], None).is_err());
    let granted = secrets_manifest_text(Some(8087), "");
    assert!(crate::package_plugin(granted.as_bytes(), &component, &[], None).is_ok());
}

/// The manifest of a secret source: no older contract, no file claimed, no viewer section, no
/// grant but `net_http`, and that one in its shape.
#[test]
fn a_secret_source_manifest_is_checked() {
    let valid = secrets_manifest_text(Some(8087), "");
    assert!(parse_manifest(valid.as_bytes()).is_ok());
    for (from, to) in [
        (
            format!("api_version = \"{}\"", rc_plugin_api::API_VERSION),
            "api_version = \"0.2.0\"".to_owned(),
        ),
        (
            "net_http = { loopback = 8087 }".to_owned(),
            "net_http = { loopback = 0 }".to_owned(),
        ),
        (
            "net_http = { loopback = 8087 }".to_owned(),
            "net_http = { domains = [\"example.net\"] }".to_owned(),
        ),
        (
            "net_http = { loopback = 8087 }".to_owned(),
            "tool = { binary = \"bw\" }".to_owned(),
        ),
        (
            "[capabilities]".to_owned(),
            "[match]\nextensions = [\"kdbx\"]\n\n[capabilities]".to_owned(),
        ),
    ] {
        let text = valid.replace(&from, &to);
        assert_ne!(text, valid, "{from}");
        assert!(parse_manifest(text.as_bytes()).is_err(), "{to}");
    }
    // A viewer is never granted `http`.
    let viewer = crate::test_fixtures::viewer_manifest_text(&["text"], "").replace(
        "[capabilities]",
        "[capabilities]\nnet_http = { loopback = 8087 }",
    );
    assert!(parse_manifest(viewer.as_bytes()).is_err());
}
