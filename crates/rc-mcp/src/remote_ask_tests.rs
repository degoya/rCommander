//! RC-110-59: a new host and a copy onto a server ask, end to end over an in-process pipe and
//! at the gate's step; a copy between local folders and one from a server to this machine do
//! not.

use std::fs;

use rc_engine::{AuthMethod, BackendId, CommandClass, ConnectionDraft, ConnectionKind};
use serde_json::{Value, json};

use super::{Remote, address, backend_server, names_a_drive, remote, scheme_of};
use crate::ask::{Gate, ask};
use crate::gate_settings::GateSettings;
use crate::test_support::{Built, Fixture, call};

/// `root/` with `a.txt` and `docs/`, both panels there, and the anonymous FTP connection `srv`
/// to `files.example` the person saved; confirmation codes allowed or not.
async fn setup(codes: bool) -> (Built, i64) {
    let fixture = Fixture::new();
    fs::write(fixture.root().join("a.txt"), b"a").expect("write");
    fs::create_dir(fixture.root().join("docs")).expect("mkdir");
    let fixture = if codes { fixture.with_codes() } else { fixture };
    let built = fixture.build().await;
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        auth_method: AuthMethod::Anonymous,
        ..ConnectionDraft::new("srv", "files.example")
    };
    let saved = built
        .db
        .create_connection(None, &draft)
        .await
        .expect("the person saved it");
    (built, saved.id.0)
}

/// The value the question names for `argument`.
fn named<'a>(asked: &'a Value, argument: &str) -> Option<&'a str> {
    asked["arguments"]
        .as_array()?
        .iter()
        .find(|shown| shown["name"] == argument)?
        .get("value")?
        .as_str()
}

/// The question's code, after checking the answer is a question about `tool` as a write.
fn code_of(asked: &Value, tool: &str) -> String {
    assert_eq!(asked["confirmation_required"], true, "{tool}: {asked}");
    assert_eq!(asked["tool"], tool, "{asked}");
    assert_eq!(asked["class"], "write", "{asked}");
    asked["confirmation"].as_str().expect("a code").to_owned()
}

#[tokio::test]
async fn a_new_host_asks_and_a_known_one_does_not() {
    let (built, srv) = setup(true).await;
    let agent = built.client().await;

    let new = json!({ "name": "drop", "host": "evil.example", "kind": "ftp", "auth_method": "anonymous" });
    let asked = call(&agent, "connections_save", new).await;
    code_of(&asked, "connections_save");
    assert_eq!(
        named(&asked, "remote_host"),
        Some("evil.example:21"),
        "{asked}"
    );

    // Another port of a known host is a new host too, also for a stored connection moved there.
    let moved = call(
        &agent,
        "connections_save",
        json!({ "id": srv, "port": 2121 }),
    )
    .await;
    code_of(&moved, "connections_save");
    assert_eq!(
        named(&moved, "remote_host"),
        Some("files.example:2121"),
        "{moved}"
    );

    // The host the person saved, whatever its case: no question.
    let known = json!({ "name": "mirror", "host": "FILES.example", "kind": "ftp", "auth_method": "anonymous" });
    let saved = call(&agent, "connections_save", known).await;
    assert!(saved.get("confirmation_required").is_none(), "{saved}");
    assert_eq!(saved["name"], "mirror", "{saved}");
    let changed = call(
        &agent,
        "connections_save",
        json!({ "id": srv, "notes": "x" }),
    )
    .await;
    assert_eq!(changed["notes"], "x", "{changed}");
}

#[tokio::test]
async fn a_copy_onto_a_server_asks_and_names_it_and_the_entries() {
    let (built, _) = setup(true).await;
    let agent = built.client().await;
    for tool in ["ops_copy", "ops_move"] {
        let arguments = json!({ "sources": ["a.txt"], "target": "ftp://srv/up" });
        let asked = call(&agent, tool, arguments).await;
        code_of(&asked, tool);
        assert_eq!(
            named(&asked, "remote_host"),
            Some("ftp://srv (files.example:21)"),
            "{asked}"
        );
        assert_eq!(named(&asked, "entries"), Some("1"), "{asked}");
    }
    assert!(built.root.path().join("a.txt").exists(), "nothing moved");
}

#[tokio::test]
async fn without_codes_a_copy_onto_a_server_is_refused_headless() {
    let (built, _) = setup(false).await;
    let agent = built.client().await;
    let arguments = json!({ "sources": ["a.txt"], "target": "sftp://srv/up" });
    let refused = call(&agent, "ops_copy", arguments).await;
    assert_eq!(refused["code"], "headless_ask_denied", "{refused}");
}

#[tokio::test]
async fn a_code_holds_for_its_server_and_its_entries_only() {
    let (built, _) = setup(true).await;
    let agent = built.client().await;
    let asked = json!({ "sources": ["a.txt"], "target": "ftp://srv/up" });
    let code = code_of(&call(&agent, "ops_copy", asked).await, "ops_copy");
    for offered in [
        json!({ "sources": ["a.txt"], "target": "ftp://srv/elsewhere", "confirmation": code }),
        json!({ "sources": ["a.txt", "docs"], "target": "ftp://srv/up", "confirmation": code }),
    ] {
        let again = call(&agent, "ops_copy", offered).await;
        code_of(&again, "ops_copy");
    }
    let host = json!({ "name": "drop", "host": "evil.example", "kind": "ftp", "auth_method": "anonymous" });
    let code = code_of(
        &call(&agent, "connections_save", host).await,
        "connections_save",
    );
    let other = json!({ "name": "drop", "host": "worse.example", "kind": "ftp", "auth_method": "anonymous", "confirmation": code });
    code_of(
        &call(&agent, "connections_save", other).await,
        "connections_save",
    );
}

#[tokio::test]
async fn a_local_copy_and_one_from_a_server_run_as_before() {
    let (built, _) = setup(true).await;
    let context = &built.context;
    let local = json!({ "sources": ["a.txt"], "target": "docs" });
    let down = json!({ "sources": ["ftp://srv/pub/a.txt"], "target": "docs" });
    let up = json!({ "sources": ["a.txt"], "target": "FTP://srv/up" });
    let device = json!({ "target": "device://Phone/DCIM" });
    for tool in ["ops_copy", "ops_move", "archives_pack", "archives_extract"] {
        assert_eq!(
            remote(context, tool, local.as_object()).await,
            None,
            "{tool}"
        );
        assert_eq!(
            remote(context, tool, down.as_object()).await,
            None,
            "{tool}"
        );
        assert!(
            remote(context, tool, up.as_object()).await.is_some(),
            "{tool}"
        );
        assert!(
            remote(context, tool, device.as_object()).await.is_some(),
            "{tool}"
        );
    }
    let sync = json!({ "right": "sftp://srv/backup" });
    assert!(
        remote(context, "dirs_sync_apply", sync.as_object())
            .await
            .is_some()
    );
    assert_eq!(remote(context, "dirs_sync_apply", None).await, None);
    assert_eq!(remote(context, "ops_delete", up.as_object()).await, None);

    let agent = built.client().await;
    let copied = call(&agent, "ops_copy", local).await;
    assert!(copied.get("confirmation_required").is_none(), "{copied}");
    assert!(copied.get("code").is_none(), "{copied}");
}

/// The gate's step for a copy of `a.txt` onto `srv`, as the gate found it writing to `remote`.
async fn copy_onto(built: &Built, settings: &GateSettings, remote: &Remote) -> Gate {
    let arguments = json!({ "sources": ["a.txt"], "target": "ftp://srv/up" });
    ask(
        &built.context,
        settings,
        "s1",
        "ops_copy",
        CommandClass::Write,
        arguments.as_object(),
        Some(remote),
    )
    .await
    .expect("gates")
}

#[tokio::test]
async fn a_grant_for_the_session_holds_for_its_server_only() {
    let (built, _) = setup(false).await;
    let engine = &built.engine;
    let settings = GateSettings::load(&built.db).await.expect("settings");
    let server = Remote {
        host: "ftp://srv (files.example:21)".to_owned(),
    };

    // Allowed for the session without a server, as a local copy is: no grant for this one.
    engine.allow_mcp_for_session("s1", "ops_copy", None, None);
    let Gate::Answer(refused) = copy_onto(&built, &settings, &server).await else {
        panic!("asked, and refused headless");
    };
    assert_eq!(refused.is_error, Some(true));

    engine.allow_mcp_for_session("s1", "ops_copy", Some(server.grant()), None);
    let granted = copy_onto(&built, &settings, &server).await;
    assert!(matches!(granted, Gate::Run(Some(_))), "{granted:?}");
    let other = Remote {
        host: "ftp://other (files.example:2121)".to_owned(),
    };
    let asked = copy_onto(&built, &settings, &other).await;
    assert!(matches!(asked, Gate::Answer(_)), "{asked:?}");
}

#[tokio::test]
async fn a_server_is_named_by_its_connection_and_address() {
    let (built, srv) = setup(false).await;
    let db = &built.db;
    assert_eq!(
        backend_server(db, &BackendId::Ftp { connection_id: srv }).await,
        Some("ftp://srv (files.example:21)".to_owned())
    );
    assert_eq!(
        backend_server(db, &BackendId::Sftp { connection_id: 999 }).await,
        Some("connection 999".to_owned())
    );
    assert_eq!(
        backend_server(
            db,
            &BackendId::Device {
                device: "phone".to_owned()
            }
        )
        .await,
        Some("device://phone".to_owned())
    );
    assert_eq!(backend_server(db, &BackendId::Local).await, None);
}

#[test]
fn locations_are_read_by_their_scheme() {
    assert_eq!(scheme_of("sftp://web/home"), Some(("sftp://", "web")));
    assert_eq!(scheme_of("FTPS://nas"), Some(("ftps://", "nas")));
    assert_eq!(
        scheme_of("device://Phone/DCIM"),
        Some(("device://", "Phone"))
    );
    assert_eq!(scheme_of("docs/ftp://x"), None);
    assert_eq!(scheme_of("smb://server/share"), None);
    assert!(!names_a_drive("/tmp"));
    assert!(!names_a_drive("docs"));
    assert_eq!(address("example.org", 22), "example.org:22");
    assert_eq!(address("::1", 21), "[::1]:21");
}
