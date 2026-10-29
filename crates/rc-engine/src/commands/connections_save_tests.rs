//! `connections.save`: every part stored at once, and a refusal after the fields - a blank or a
//! wrong secret, a locked vault, a missing script - leaves the stored connection exactly as it
//! was (RC-080-25).

use std::{path::Path, sync::Arc, time::Duration};

use rc_db::{DB_FILE, Db, SECRET_COLUMNS};
use rc_vault::{ExposeSecret as _, KdfParams, ManualClock, Vault, VaultOptions};
use rc_vfs::VfsPath;
use tokio::sync::broadcast::Receiver;

use crate::{
    AuthMethod, Command, Connection, ConnectionDraft, ConnectionId, ConnectionPatch, Engine,
    EngineError, EngineEvent, LoginScriptDraft, LoginScriptId, Outcome, SaveConnectionRequest,
    SaveFields, SecretChange, SecretKind, SecretParam, SecretRef, Step, TreeChange,
};

const MASTER: &str = "correct horse battery";
const PASSWORD: &str = "canary-save-password";
/// `rc-remote`'s throw-away test keys.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../rc-remote/tests/fixtures");

/// An engine on the database file in `dir` with an unlocked vault that derives in milliseconds.
async fn engine_in(dir: &Path) -> Engine {
    let db = Db::open(dir.join(DB_FILE)).await.expect("db");
    let engine = Engine::headless(VfsPath::local(dir)).await.expect("engine");
    engine.attach_db(db.clone());
    let vault = Vault::open_with(
        db,
        VaultOptions {
            clock: Arc::new(ManualClock::new()),
            kdf: KdfParams {
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            },
            failure_delay: Duration::ZERO,
            secret_columns: SECRET_COLUMNS,
        },
    )
    .await
    .expect("vault");
    assert!(engine.attach_vault(vault));
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault created");
    engine
}

fn create(draft: ConnectionDraft) -> SaveConnectionRequest {
    SaveConnectionRequest {
        fields: SaveFields::Create {
            folder_id: None,
            connection: draft,
        },
        secrets: Vec::new(),
        login_script: None,
        favourite: None,
        credential: None,
    }
}

fn update(id: ConnectionId, patch: ConnectionPatch) -> SaveConnectionRequest {
    SaveConnectionRequest {
        fields: SaveFields::Update { id, patch },
        ..create(ConnectionDraft::new("unused", "example.org"))
    }
}

fn notes(text: &str) -> ConnectionPatch {
    ConnectionPatch {
        notes: Some(text.into()),
        ..ConnectionPatch::default()
    }
}

fn set(kind: SecretKind, value: &str) -> SecretChange {
    SecretChange::Set {
        kind,
        value: SecretParam::from(value),
    }
}

async fn save(engine: &Engine, request: SaveConnectionRequest) -> Result<Connection, EngineError> {
    match engine.dispatch(Command::SaveConnection(request)).await? {
        Outcome::Connection { connection } => Ok(connection),
        other => panic!("a connection, got {other:?}"),
    }
}

async fn stored(engine: &Engine, id: ConnectionId) -> Connection {
    match engine.dispatch(Command::GetConnection { id }).await {
        Ok(Outcome::Connection { connection }) => connection,
        other => panic!("a connection, got {other:?}"),
    }
}

async fn secret(engine: &Engine, id: ConnectionId, kind: SecretKind) -> Option<String> {
    engine
        .resolve_secret(SecretRef {
            connection: id,
            kind,
        })
        .await
        .expect("resolves")
        .map(|secret| secret.expose_secret().to_owned())
}

async fn script(engine: &Engine) -> LoginScriptId {
    let draft = LoginScriptDraft {
        name: "enter".into(),
        steps: vec![Step::SendLine],
        step_timeout_ms: None,
        total_timeout_ms: None,
    };
    match engine
        .dispatch(Command::CreateLoginScript { script: draft })
        .await
    {
        Ok(Outcome::LoginScript { script }) => script.id,
        other => panic!("a login script, got {other:?}"),
    }
}

fn drain(events: &mut Receiver<EngineEvent>) -> Vec<EngineEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

#[tokio::test]
async fn one_save_stores_the_fields_secret_script_and_mark_with_one_tree_event() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let script = script(&engine).await;
    let mut events = engine.subscribe();
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, PASSWORD)],
        login_script: Some(Some(script)),
        // A root connection is listed by default; the mark takes it off the drive bar.
        favourite: Some(false),
        ..create(ConnectionDraft::new("web", "example.org"))
    };
    let saved = save(&engine, request).await.expect("saved");
    assert_eq!(saved.name, "web");
    assert!(saved.secrets.has_password);
    assert_eq!(saved.login_script_id, Some(script.0));
    assert_eq!(stored(&engine, saved.id).await, saved);
    assert_eq!(
        secret(&engine, saved.id, SecretKind::Password).await,
        Some(PASSWORD.to_owned())
    );
    assert_eq!(engine.favourites().await.hidden, vec![saved.id]);
    let events = drain(&mut events);
    let tree: Vec<&EngineEvent> = events
        .iter()
        .filter(|event| matches!(event, EngineEvent::ConnectionTree { .. }))
        .collect();
    assert!(
        matches!(
            tree.as_slice(),
            [EngineEvent::ConnectionTree {
                change: TreeChange::Created,
                ..
            }]
        ),
        "one save, one change of the tree: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::Drives))
    );
    // The value never shows in the command or the answer.
    let command = Command::SaveConnection(SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, PASSWORD)],
        ..update(saved.id, ConnectionPatch::default())
    });
    assert!(!format!("{command:?}").contains(PASSWORD));
    assert!(
        !serde_json::to_string(&command)
            .expect("json")
            .contains(PASSWORD)
    );
    assert!(!format!("{saved:?}").contains(PASSWORD));
}

#[tokio::test]
async fn a_refusal_after_the_fields_leaves_the_connection_as_it_was() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Key,
        ..ConnectionDraft::new("web", "example.org")
    };
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, PASSWORD)],
        ..create(draft)
    };
    let id = save(&engine, request).await.expect("saved").id;
    let before = stored(&engine, id).await;

    let refusals = [
        (vec![set(SecretKind::Password, "   ")], "secret_empty"),
        (vec![set(SecretKind::Key, "a key")], "key_import_only"),
        (
            vec![SecretChange::ImportKey {
                path: format!("{FIXTURES}/client_key_rsapem"),
                passphrase: Some(SecretParam::from("not the passphrase")),
            }],
            "key_passphrase_wrong",
        ),
        (
            vec![SecretChange::ImportKey {
                path: format!("{FIXTURES}/no_such_key"),
                passphrase: None,
            }],
            "key_file_unreadable",
        ),
    ];
    for (secrets, code) in refusals {
        let request = SaveConnectionRequest {
            secrets,
            ..update(id, notes("changed"))
        };
        let refused = save(&engine, request).await.expect_err("refused");
        assert_eq!(refused.code(), code);
        assert_eq!(stored(&engine, id).await, before, "after {code}");
    }

    let request = SaveConnectionRequest {
        secrets: vec![SecretChange::Clear {
            kind: SecretKind::Password,
        }],
        login_script: Some(Some(LoginScriptId(99))),
        ..update(id, notes("changed"))
    };
    let refused = save(&engine, request).await.expect_err("refused");
    assert_eq!(refused.code(), "login_script_not_found");
    assert_eq!(stored(&engine, id).await, before, "the clear went too");

    engine.dispatch(Command::LockVault).await.expect("locked");
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, "a new one")],
        ..update(id, notes("changed"))
    };
    let refused = save(&engine, request).await.expect_err("refused");
    assert_eq!(refused.code(), "vault_locked");
    assert_eq!(stored(&engine, id).await, before);

    // A new connection refused at its secret is not created at all.
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, "a new one")],
        ..create(ConnectionDraft::new("db", "example.org"))
    };
    assert_eq!(
        save(&engine, request).await.expect_err("refused").code(),
        "vault_locked"
    );
    let Ok(Outcome::ConnectionTree { tree }) = engine.dispatch(Command::ListConnections).await
    else {
        panic!("the tree");
    };
    assert_eq!(tree.connections.len(), 1);

    // A field's refusal comes first, as in the form.
    let blank = ConnectionPatch {
        name: Some(String::new()),
        ..ConnectionPatch::default()
    };
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, "   ")],
        ..update(id, blank)
    };
    assert_eq!(
        save(&engine, request).await.expect_err("refused").code(),
        "name_empty"
    );
}

#[tokio::test]
async fn an_encrypted_key_is_taken_in_and_a_cleared_secret_goes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Key,
        ..ConnectionDraft::new("web", "example.org")
    };
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, PASSWORD)],
        ..create(draft)
    };
    let id = save(&engine, request).await.expect("saved").id;
    let mut events = engine.subscribe();
    let request = SaveConnectionRequest {
        secrets: vec![
            SecretChange::Clear {
                kind: SecretKind::Password,
            },
            SecretChange::ImportKey {
                path: format!("{FIXTURES}/client_key_rsapem"),
                passphrase: Some(SecretParam::from("fixture-passphrase")),
            },
        ],
        ..update(id, ConnectionPatch::default())
    };
    let saved = save(&engine, request).await.expect("saved");
    assert!(!saved.secrets.has_password);
    assert!(saved.secrets.has_key && saved.secrets.has_passphrase);
    assert_eq!(
        saved.key.as_ref().map(|key| key.fingerprint.as_str()),
        Some("SHA256:6KajqcW1Orb7TSswcvfa2oFhUNaLC46DsrFV+00r028")
    );
    assert_eq!(
        secret(&engine, id, SecretKind::Passphrase).await.as_deref(),
        Some("fixture-passphrase")
    );

    // Removing every secret needs no vault and tells the window the vault is unused.
    engine.dispatch(Command::LockVault).await.expect("locked");
    drain(&mut events);
    let request = SaveConnectionRequest {
        secrets: vec![
            SecretChange::Clear {
                kind: SecretKind::Key,
            },
            SecretChange::Clear {
                kind: SecretKind::Passphrase,
            },
        ],
        ..update(id, ConnectionPatch::default())
    };
    let cleared = save(&engine, request).await.expect("cleared while locked");
    assert!(!cleared.secrets.has_key && !cleared.secrets.has_passphrase);
    assert_eq!(cleared.key, None);
    assert!(
        drain(&mut events)
            .iter()
            .any(|event| matches!(event, EngineEvent::VaultUnused))
    );
}

#[tokio::test]
async fn over_mcp_a_save_changes_no_secret_and_no_endpoint_that_holds_one() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let request = SaveConnectionRequest {
        secrets: vec![set(SecretKind::Password, PASSWORD)],
        ..create(ConnectionDraft::new("web", "example.org"))
    };
    let id = save(&engine, request).await.expect("saved").id;
    let before = stored(&engine, id).await;

    let with_secret = SaveConnectionRequest {
        secrets: vec![SecretChange::Clear {
            kind: SecretKind::Password,
        }],
        ..update(id, notes("changed"))
    };
    let refused = engine
        .dispatch_mcp(Command::SaveConnection(with_secret))
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "gui_only");
    let moved = ConnectionPatch {
        host: Some("elsewhere.example.org".into()),
        ..ConnectionPatch::default()
    };
    let refused = engine
        .dispatch_mcp(Command::SaveConnection(update(id, moved)))
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "endpoint_change_needs_gui");
    assert_eq!(stored(&engine, id).await, before);

    let request = SaveConnectionRequest {
        favourite: Some(false),
        ..update(id, notes("from an agent"))
    };
    let Ok(Outcome::Connection { connection }) =
        engine.dispatch_mcp(Command::SaveConnection(request)).await
    else {
        panic!("saved over MCP");
    };
    assert_eq!(connection.notes.as_deref(), Some("from an agent"));
    assert!(connection.secrets.has_password, "the secret stays");
    assert_eq!(engine.favourites().await.hidden, vec![id]);
}
