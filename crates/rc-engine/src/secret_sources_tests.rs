//! Secrets taken from a source (RC-100-34), over an installed test source: the value reaches the
//! caller and nothing that stays or leaves; each way a source fails is named; without a
//! reference nothing changes; MCP can neither set a reference nor move a mapped connection.

use rc_db::{
    Db,
    connections::{ConnectionDraft, ConnectionPatch},
};
use rc_plugin_host::{
    key_fingerprint,
    test_support::{secrets_package, signing_key},
    testbed::{
        SPIN, TRAP,
        secrets::{CANARY, Export, LOCKED, SecretsBed},
    },
};
use rc_vault::ExposeSecret as _;

use super::*;
use crate::{Command, ConnectionId, Outcome, SecretRef, test_support};

const ID: &str = "019d0000-0000-7000-8000-0000000000b1";
const SOURCE: &str = "plugin:019d0000-0000-7000-8000-0000000000b1";
const UNREACHABLE: u8 = 2;
const UNLICENSED: u8 = 3;

pub(super) struct Setup {
    root: tempfile::TempDir,
    pub(super) engine: Engine,
    pub(super) db: Db,
    pub(super) connection: ConnectionId,
}

impl Setup {
    /// An engine with a database file, the source of `bed` as its only one, and a connection.
    pub(super) async fn new(bed: &SecretsBed) -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let engine = test_support::headless(root.path()).await;
        let db = Db::open(root.path().join("rcommander.db"))
            .await
            .expect("db");
        assert!(engine.attach_db(db.clone()));
        engine
            .set_protected_setting(WINCRED_SOURCE_KEY, &false.into())
            .await
            .expect("off");
        assert!(engine.attach_plugins(root.path().join("data"), None));
        let package = root.path().join("source.rcplug");
        let key = signing_key(9);
        let bytes = secrets_package(ID, "tests-v1", &key, &bed.build());
        std::fs::write(&package, &bytes).expect("write");
        engine
            .dispatch(Command::InstallPlugin {
                path: package.display().to_string(),
                fingerprint: Some(key_fingerprint(&key.verifying_key())),
                digest: rc_plugin_host::sha256_hex(&bytes),
            })
            .await
            .expect("the window installs");
        let connection = db
            .create_connection(None, &ConnectionDraft::new("web", "example.org"))
            .await
            .expect("create")
            .id;
        Self {
            root,
            engine,
            db,
            connection,
        }
    }

    fn reference(&self) -> SecretSourceRef {
        SecretSourceRef {
            connection_id: self.connection,
            kind: SecretKind::Password,
            source: SOURCE.to_owned(),
            entry_id: "7c17".to_owned(),
            entry_label: "web (root)".to_owned(),
        }
    }

    pub(super) async fn map(&self) {
        self.engine
            .dispatch(Command::MapSecretSource(self.reference()))
            .await
            .expect("mapped");
    }

    async fn resolve(&self) -> Result<Option<rc_vault::SecretString>, EngineError> {
        self.engine
            .resolve_secret(SecretRef {
                connection: self.connection,
                kind: SecretKind::Password,
            })
            .await
    }

    /// The code of a failed resolve, and its message.
    pub(super) async fn failure(&self) -> (&'static str, String) {
        let error = self.resolve().await.expect_err("the source fails");
        (error.code(), error.to_string())
    }
}

/// The canary: the value the source hands out is the answer and nothing else - not the database
/// file, not an event, not an error or `Debug`; an MCP answer or a terminal screen holding it
/// is scrubbed.
#[tokio::test]
async fn the_value_reaches_the_login_and_nothing_that_stays() {
    let setup = Setup::new(&SecretsBed::new()).await;
    let mut events = setup.engine.subscribe();
    setup.map().await;
    let value = setup.resolve().await.expect("resolved").expect("a value");
    assert_eq!(value.expose_secret(), CANARY);
    assert!(!format!("{value:?}").contains(CANARY));
    // Asked again at the next connect: nothing is kept between them.
    assert_eq!(
        setup
            .resolve()
            .await
            .expect("again")
            .expect("a value")
            .expose_secret(),
        CANARY
    );

    let scrubbed = setup
        .engine
        .scrub_secrets(&format!("$ echo {CANARY}\n{CANARY}"))
        .expect("the canary is known");
    assert!(!scrubbed.contains(CANARY), "{scrubbed}");

    while let Ok(event) = events.try_recv() {
        assert!(!format!("{event:?}").contains(CANARY), "{event:?}");
    }
    let listed = setup
        .engine
        .dispatch(Command::ListSecretSources {
            connection_id: Some(setup.connection),
        })
        .await
        .expect("listed");
    assert!(!format!("{listed:?}").contains(CANARY), "{listed:?}");

    drop(setup.engine);
    setup.db.close().await;
    let mut file = std::fs::read(setup.root.path().join("rcommander.db")).expect("db file");
    if let Ok(wal) = std::fs::read(setup.root.path().join("rcommander.db-wal")) {
        file.extend(wal);
    }
    let canary = CANARY.as_bytes();
    assert!(
        !file.windows(canary.len()).any(|window| window == canary),
        "the value is in the database"
    );
}

#[tokio::test]
async fn a_source_that_cannot_answer_says_why_and_never_hangs() {
    let started = std::time::Instant::now();
    let cases = [
        (
            SecretsBed::new()
                .body(Export::Status, format!("(i32.const {LOCKED})"))
                .body(Export::Get, TRAP),
            "secret_source_locked",
        ),
        (
            SecretsBed::new()
                .body(Export::Status, format!("(i32.const {UNREACHABLE})"))
                .body(Export::Get, TRAP),
            "secret_source_unreachable",
        ),
        (
            SecretsBed::new()
                .body(Export::Status, format!("(i32.const {UNLICENSED})"))
                .body(Export::Get, TRAP),
            "secret_source_unlicensed",
        ),
        (
            SecretsBed::new().body(Export::Get, TRAP),
            "secret_source_failed",
        ),
        (
            SecretsBed::new().body(Export::Get, SPIN),
            "secret_source_failed",
        ),
    ];
    for (bed, expected) in cases {
        let setup = Setup::new(&bed).await;
        setup.map().await;
        let (code, message) = setup.failure().await;
        assert_eq!(code, expected, "{message}");
        assert!(message.contains("Test source"), "{message}");
        // The connection stays editable.
        setup
            .engine
            .dispatch(Command::UpdateConnection {
                id: setup.connection,
                patch: ConnectionPatch {
                    notes: Some("still editable".into()),
                    ..ConnectionPatch::default()
                },
            })
            .await
            .expect("editable");
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

#[tokio::test]
async fn a_source_that_is_gone_is_named_at_connect() {
    let setup = Setup::new(&SecretsBed::new()).await;
    setup.map().await;
    setup
        .engine
        .dispatch(Command::DisablePlugin { id: ID.to_owned() })
        .await
        .expect("disabled");
    let (code, message) = setup.failure().await;
    assert_eq!(code, "secret_source_missing");
    assert!(message.contains(SOURCE), "{message}");
    // A missing source cannot be mapped either (wincred is switched off).
    for source in [SOURCE, "wincred"] {
        let refused = setup
            .engine
            .dispatch(Command::MapSecretSource(SecretSourceRef {
                source: source.to_owned(),
                ..setup.reference()
            }))
            .await
            .expect_err("missing");
        assert_eq!(refused.code(), "secret_source_missing");
    }
}

/// Without a reference nothing changes: no value stored answers `None`, and the vault is never
/// asked for (`vault_absent` would be an error).
#[tokio::test]
async fn without_a_reference_the_vault_is_asked_as_before() {
    let setup = Setup::new(&SecretsBed::new()).await;
    assert!(setup.resolve().await.expect("nothing stored").is_none());
    setup.map().await;
    setup
        .engine
        .dispatch(Command::UnmapSecretSource {
            connection_id: setup.connection,
            kind: SecretKind::Password,
        })
        .await
        .expect("unmapped");
    assert!(setup.resolve().await.expect("nothing stored").is_none());
}

#[tokio::test]
async fn a_reference_is_checked_before_it_is_stored() {
    let setup = Setup::new(&SecretsBed::new()).await;
    for (entry_id, entry_label) in [
        (String::new(), "label".to_owned()),
        ("a".repeat(257), "label".to_owned()),
        ("7c17".to_owned(), "x".repeat(513)),
        ("7c\n17".to_owned(), "label".to_owned()),
        ("7c17".to_owned(), "a\u{1b}[2J".to_owned()),
    ] {
        let refused = setup
            .engine
            .dispatch(Command::MapSecretSource(SecretSourceRef {
                entry_id,
                entry_label,
                ..setup.reference()
            }))
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), "secret_source_invalid");
    }
    assert_eq!(
        setup
            .db
            .secret_sources(setup.connection)
            .await
            .expect("read"),
        Vec::new()
    );
}

/// MCP lists and searches, never sets or removes a reference; saving a connection without
/// secrets leaves its reference alone, and its endpoint stays the window's.
#[tokio::test]
async fn mcp_cannot_set_or_move_a_reference() {
    let setup = Setup::new(&SecretsBed::new()).await;
    let engine = &setup.engine;
    for command in [
        Command::MapSecretSource(setup.reference()),
        Command::UnmapSecretSource {
            connection_id: setup.connection,
            kind: SecretKind::Password,
        },
    ] {
        let refused = engine.dispatch_mcp(command).await.expect_err("GUI only");
        assert_eq!(refused.code(), "gui_only");
    }
    setup.map().await;

    let listed = engine
        .dispatch_mcp(Command::ListSecretSources {
            connection_id: Some(setup.connection),
        })
        .await
        .expect("a read");
    let Outcome::SecretSources(outcome) = listed else {
        panic!("sources, got {listed:?}");
    };
    let SecretSourcesOutcome::Sources { sources, mappings } = *outcome else {
        panic!("sources");
    };
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].source, SOURCE);
    assert_eq!(sources[0].state, "ready");
    assert_eq!(mappings, vec![setup.reference()]);
    let searched = engine
        .dispatch_mcp(Command::SearchSecretSource {
            source: SOURCE.to_owned(),
            query: "web".to_owned(),
        })
        .await
        .expect("a read");
    assert!(
        matches!(searched, Outcome::SecretSources(_)),
        "{searched:?}"
    );

    engine
        .dispatch_mcp(Command::UpdateConnection {
            id: setup.connection,
            patch: ConnectionPatch {
                notes: Some("from an agent".into()),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect("notes change");
    assert_eq!(
        setup
            .db
            .secret_sources(setup.connection)
            .await
            .expect("read"),
        vec![setup.reference()]
    );
    let moved = engine
        .dispatch_mcp(Command::UpdateConnection {
            id: setup.connection,
            patch: ConnectionPatch {
                host: Some("elsewhere.example".into()),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect_err("the endpoint is the window's");
    assert_eq!(moved.code(), "endpoint_change_needs_gui");
}

/// An export carries the reference and never the source's value; an import on another machine
/// stores it even without the source, which a connect then names.
#[tokio::test]
async fn the_reference_travels_with_an_export() {
    let setup = Setup::new(&SecretsBed::new()).await;
    setup.map().await;
    let path = setup.root.path().join("all.rcexport");
    setup
        .engine
        .dispatch(Command::ExportConfig(crate::ConfigExportRequest {
            path: path.display().to_string(),
            parts: crate::exchange::ExportParts::ALL,
            password: None,
            secrets: false,
            plain_secrets: false,
            overwrite: false,
        }))
        .await
        .expect("exported");
    let file = std::fs::read_to_string(&path).expect("the export");
    assert!(file.contains("7c17") && file.contains(SOURCE), "{file}");
    assert!(!file.contains(CANARY));

    let other = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless_with_db(other.path()).await;
    engine
        .dispatch(Command::ImportConfig(crate::ConfigImportRequest {
            path: path.display().to_string(),
            ..crate::ConfigImportRequest::default()
        }))
        .await
        .expect("imported");
    // Not chosen by itself: the row waits for the person's tick.
    let db = engine.db().expect("db");
    assert!(
        db.connection_tree()
            .await
            .expect("tree")
            .connections
            .is_empty()
    );
    let Outcome::ConfigImported(preview) = engine
        .dispatch(Command::ImportConfig(crate::ConfigImportRequest {
            path: path.display().to_string(),
            dry_run: true,
            ..crate::ConfigImportRequest::default()
        }))
        .await
        .expect("preview")
    else {
        panic!("a report");
    };
    let row = preview
        .rows
        .iter()
        .find(|row| row.warning.as_deref() == Some("secret_sources"))
        .expect("the connection's row warns");
    engine
        .dispatch(Command::ImportConfig(crate::ConfigImportRequest {
            path: path.display().to_string(),
            only: Some(vec![row.key.clone()]),
            ..crate::ConfigImportRequest::default()
        }))
        .await
        .expect("imported when ticked");
    let imported = db.connection_tree().await.expect("tree").connections[0].id;
    let references = db.secret_sources(imported).await.expect("read");
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].entry_id, "7c17");
    let missing = engine
        .resolve_secret(SecretRef {
            connection: imported,
            kind: SecretKind::Password,
        })
        .await
        .expect_err("no source here");
    assert_eq!(missing.code(), "secret_source_missing");
}

/// A masked setting - a licence key - reaches the window and the plugin, never an MCP answer
/// (RC-100-36).
#[tokio::test]
async fn a_masked_setting_never_reaches_mcp() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless_with_db(root.path()).await;
    assert!(engine.attach_plugins(root.path().join("data"), None));
    let key = signing_key(9);
    let package = root.path().join("source.rcplug");
    std::fs::write(
        &package,
        rc_plugin_host::test_support::secrets_package_with(
            ID,
            "tests-v1",
            &key,
            &SecretsBed::new().build(),
            "\n[[settings]]\nkey = \"licence_key\"\ntype = \"text\"\nmasked = true\n",
        ),
    )
    .expect("write");
    engine
        .dispatch(Command::InstallPlugin {
            path: package.display().to_string(),
            fingerprint: Some(key_fingerprint(&key.verifying_key())),
            digest: rc_plugin_host::sha256_hex(&std::fs::read(&package).expect("read")),
        })
        .await
        .expect("installed");
    engine
        .set_plugin_setting(ID, "licence_key", "a canary licence")
        .await
        .expect("stored");
    let value = |outcome: Outcome| match outcome {
        Outcome::Plugins(outcome) => match *outcome {
            crate::PluginsOutcome::List { plugins, .. } => plugins[0].settings[0].clone(),
            other => panic!("a list, got {other:?}"),
        },
        other => panic!("plugins, got {other:?}"),
    };
    let window = value(
        engine
            .dispatch(Command::ListPlugins { id: None })
            .await
            .expect("listed"),
    );
    assert!(window.masked);
    assert_eq!(window.value.as_deref(), Some("a canary licence"));
    let agent = engine
        .dispatch_mcp(Command::ListPlugins { id: None })
        .await
        .expect("listed");
    assert!(
        !format!("{agent:?}").contains("canary licence"),
        "{agent:?}"
    );
    assert_eq!(value(agent).value.as_deref(), Some(crate::REDACTED));
}
