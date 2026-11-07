//! The inventory kind (RC-100-39): an inventory answers its hosts, held to the host's rules; it
//! reaches the address the person entered and nothing else; its token reaches that server and
//! nobody else; a plugin of the contract before still runs.

use rc_plugin_api::inventory::Protocol;
use rc_plugin_types::{Failure, FailureKind};
use secrecy::SecretString;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
    sync::oneshot,
};

use super::{InventoryOptions, InventoryPlugin};
use crate::{
    SecretsOptions, SecretsPlugin, ViewerOptions, ViewerPlugin,
    failure::codes,
    manifest::parse_manifest,
    test_fixtures::{
        inventory_manifest, inventory_manifest_text, secrets_manifest_text, viewer_manifest_text,
    },
    testbed::{
        self, TRAP,
        inventory::{HostData, InventoryBed, SFTP, SSH, echo_answer, many_hosts, request},
        secrets::SecretsBed,
    },
};

const TOKEN: &str = "canary-token-5521";

fn options(address: Option<&str>) -> InventoryOptions {
    InventoryOptions {
        address: address.map(str::to_owned),
        secrets: vec![("api_token".to_owned(), SecretString::from(TOKEN))],
        ..InventoryOptions::default()
    }
}

fn plugin(bed: &InventoryBed, address: Option<&str>) -> InventoryPlugin {
    InventoryPlugin::new(&inventory_manifest(true), &bed.build(), options(address))
        .expect("an inventory")
}

fn code(failure: &Failure) -> Option<&str> {
    failure.code()
}

/// Two hosts, the second with every optional field left out.
fn two_hosts() -> InventoryBed {
    let mut bed = InventoryBed::new();
    let tags = [bed.text("prod"), bed.text("web")];
    let tag_list = bed.data(&testbed::list_bytes(&tags));
    let first = HostData {
        external_id: bed.text("dcim.device.17"),
        name: bed.text("web-01"),
        address: bed.text("web-01.example.net"),
        port: 22,
        protocol: SSH,
        user: Some(bed.text("deploy")),
        folder: bed.text("dc1/web"),
        // A list's length counts its strings, not its bytes.
        tags: (tag_list.0, 2),
        note: Some(bed.text("rack 4")),
    };
    let second = HostData {
        external_id: bed.text("dcim.device.18"),
        name: bed.text("files"),
        address: bed.text("10.0.0.8"),
        port: 2222,
        protocol: SFTP,
        user: None,
        folder: bed.text(""),
        tags: (0, 0),
        note: None,
    };
    bed.hosts(&[first, second])
}

#[tokio::test]
async fn an_inventory_answers_its_hosts() {
    let inventory = plugin(&two_hosts(), None);
    let list = inventory.list().await.expect("a list");
    assert_eq!(list.skipped, 0);
    assert_eq!(list.hosts.len(), 2);
    let first = &list.hosts[0];
    assert_eq!(first.external_id, "dcim.device.17");
    assert_eq!(first.address, "web-01.example.net");
    assert_eq!(first.protocol, Protocol::Ssh);
    assert_eq!(first.user.as_deref(), Some("deploy"));
    assert_eq!(first.folder, "dc1/web");
    assert_eq!(first.tags, ["prod", "web"]);
    let second = &list.hosts[1];
    assert_eq!(second.port, 2222);
    assert_eq!(second.protocol, Protocol::Sftp);
    assert_eq!(second.user, None);
    assert_eq!(second.folder, "");
}

/// A huge list is refused as a whole; a list of hosts without ids or ports is read as nothing.
#[tokio::test]
async fn a_huge_or_empty_answer_is_held_to_the_rules() {
    let huge = plugin(&InventoryBed::new().body(many_hosts(10_001)), None);
    let failure = huge.list().await.expect_err("too many");
    assert_eq!(code(&failure), Some(codes::PLUGIN_INVENTORY_TOO_LARGE));
    let blank = plugin(&InventoryBed::new().body(many_hosts(3)), None);
    let list = blank.list().await.expect("a list");
    assert!(list.hosts.is_empty());
    assert_eq!(list.skipped, 3);
}

#[tokio::test]
async fn a_crash_fails_the_one_call() {
    let crashing = plugin(&InventoryBed::new().body(TRAP), None);
    let failure = crashing.list().await.expect_err("crashed");
    assert_eq!(code(&failure), Some(codes::PLUGIN_CRASHED));
}

/// A server on a free loopback port that answers one connection with the request it read; the
/// port and the request it saw.
async fn serve_echo() -> (u16, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let (sender, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut seen = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !seen.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).await.expect("read");
            if read == 0 {
                break;
            }
            seen.extend_from_slice(&buffer[..read]);
        }
        let text = String::from_utf8_lossy(&seen).into_owned();
        let answer = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{text}",
            text.len()
        );
        stream.write_all(answer.as_bytes()).await.expect("write");
        let _ = sender.send(text);
    });
    (port, receiver)
}

/// An inventory sending `GET <url>` with `Authorization: Token {{secret:api_token}}`, then
/// logging what came back and handing it back as its failure's parameter `echo`.
fn echoing(url: &str) -> InventoryBed {
    let mut bed = InventoryBed::new();
    let method = bed.text("GET");
    let url = bed.text(url);
    let name = bed.text("Authorization");
    let value = bed.text("Token {{secret:api_token}}");
    let headers = bed.data(&testbed::list_bytes(&[name, value]));
    let headers = (headers.0, 1);
    let level = bed.text("warn");
    let key = bed.text("echo");
    let body = format!(
        "{}{}",
        request(method, url, headers),
        echo_answer(level, key)
    );
    bed.body(body)
}

/// The canary: the token reaches the server the person confirmed, in the header the plugin named
/// it in; the plugin only ever wrote the marker; what the server echoed reaches neither the
/// caller nor the execution log.
#[tokio::test]
async fn the_token_reaches_the_confirmed_server_only() {
    let (port, seen) = serve_echo().await;
    let address = format!("http://127.0.0.1:{port}/");
    let bed = echoing(&format!("{address}api/dcim/devices/"));
    assert!(!bed.core_text().contains(TOKEN));
    let inventory = plugin(&bed, Some(&address));
    let failure = inventory.list().await.expect_err("the echo");
    let seen = seen.await.expect("the request");
    assert!(seen.contains(&format!("Token {TOKEN}")), "{seen}");
    let echo = failure.params.get("echo").expect("the echo");
    assert!(echo.contains("[redacted]"), "{echo}");
    assert!(!format!("{failure:?}").contains(TOKEN));
    let records = format!(
        "{:?}",
        inventory.execution_log().records(&inventory.identity().id)
    );
    assert!(!records.contains(TOKEN), "{records}");
}

/// A marker towards another host than the confirmed one is never filled: the request is denied
/// before it leaves.
#[tokio::test]
async fn a_marker_to_another_host_is_not_filled() {
    let (port, mut seen) = serve_echo().await;
    let bed = echoing(&format!("http://127.0.0.1:{port}/api/"));
    let elsewhere = plugin(&bed, Some("https://netbox.example.net/"));
    let failure = elsewhere.list().await.expect_err("denied");
    assert_eq!(failure.kind, FailureKind::AccessDenied);
    assert!(seen.try_recv().is_err(), "nothing reached the server");
}

/// No address entered (a conformance run): every request is denied.
#[tokio::test]
async fn without_an_address_nothing_is_reached() {
    let bed = echoing("https://netbox.example.net/api/");
    let failure = plugin(&bed, None).list().await.expect_err("denied");
    assert_eq!(failure.kind, FailureKind::AccessDenied);
}

/// An address in clear over the network is refused when the source is built.
#[test]
fn a_plain_http_address_elsewhere_is_refused() {
    let refused = InventoryPlugin::new(
        &inventory_manifest(true),
        &InventoryBed::new().build(),
        options(Some("http://netbox.example.net/")),
    );
    assert_eq!(
        refused.err().as_ref().and_then(code),
        Some(codes::PLUGIN_HTTP_DENIED)
    );
}

/// An import of `http` without the grant is refused, as for a secret source.
#[test]
fn http_without_the_grant_is_refused() {
    let bed = echoing("https://netbox.example.net/");
    let refused = InventoryPlugin::new(
        &inventory_manifest(false),
        &bed.build(),
        InventoryOptions::default(),
    );
    assert_eq!(
        refused.err().as_ref().and_then(code),
        Some(codes::PLUGIN_DENIED)
    );
}

/// Contract 0.4.0 is additive: a viewer and a secret source built against 0.3.0 still run, and
/// an inventory cannot name a contract before its world.
#[tokio::test]
async fn the_contract_before_still_runs() {
    let viewer = parse_manifest(
        viewer_manifest_text(&["text"], "")
            .replace("api_version = \"0.1.0\"", "api_version = \"0.3.0\"")
            .as_bytes(),
    )
    .expect("a 0.3.0 viewer");
    ViewerPlugin::new(
        &viewer,
        &testbed::Testbed::new().contract("0.3.0").build(),
        ViewerOptions::default(),
    )
    .expect("a 0.3.0 viewer runs");
    let api = format!("api_version = \"{}\"", rc_plugin_api::API_VERSION);
    let source = parse_manifest(
        secrets_manifest_text(None, "")
            .replace(&api, "api_version = \"0.3.0\"")
            .as_bytes(),
    )
    .expect("a 0.3.0 secret source");
    let source = SecretsPlugin::new(
        &source,
        &SecretsBed::new().contract("0.3.0").build(),
        SecretsOptions::default(),
    )
    .expect("a 0.3.0 secret source runs");
    assert!(source.status().await.is_ok());
    let old = inventory_manifest_text(true, "").replace(&api, "api_version = \"0.3.0\"");
    assert!(parse_manifest(old.as_bytes()).is_err());
}
