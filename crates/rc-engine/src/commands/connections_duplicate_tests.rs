//! `connections.duplicate` (RC-050-15): the copy has every field and its own secrets sealed for
//! its row, sits right under the original as `<name> (2)`, and a locked vault or an agent's call
//! writes nothing.

use std::{path::Path, sync::Arc, time::Duration};

use rc_db::{DB_FILE, Db, SECRET_COLUMNS};
use rc_vault::{ExposeSecret as _, KdfParams, ManualClock, Vault, VaultOptions};
use rc_vfs::VfsPath;

use super::strip_number;
use crate::{
    AuthMethod, Command, Connection, ConnectionDraft, ConnectionId, Engine, EngineError, Outcome,
    SaveConnectionRequest, SaveFields, SecretChange, SecretKind, SecretParam, SecretRef,
};

const MASTER: &str = "correct horse battery";
const PASSWORD: &str = "canary-duplicate-password";
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

/// `web` at the root with a password, a stored key and its passphrase, between two others.
async fn web(engine: &Engine) -> Connection {
    for name in ["alpha", "web", "zulu"] {
        let secrets = if name == "web" {
            vec![
                SecretChange::Set {
                    kind: SecretKind::Password,
                    value: SecretParam::from(PASSWORD),
                },
                SecretChange::ImportKey {
                    path: format!("{FIXTURES}/client_key_rsapem"),
                    passphrase: Some(SecretParam::from("fixture-passphrase")),
                },
            ]
        } else {
            Vec::new()
        };
        let draft = ConnectionDraft {
            auth_method: AuthMethod::Key,
            username: Some("deploy".into()),
            port: Some(2222),
            notes: Some("the web server".into()),
            ..ConnectionDraft::new(name, "example.org")
        };
        let request = SaveConnectionRequest {
            fields: SaveFields::Create {
                folder_id: None,
                connection: draft,
            },
            secrets,
            login_script: None,
            favourite: None,
            credential: None,
        };
        engine
            .dispatch(Command::SaveConnection(request))
            .await
            .expect("saved");
    }
    root_connections(engine)
        .await
        .into_iter()
        .find(|connection| connection.name == "web")
        .expect("web")
}

async fn duplicate(engine: &Engine, id: ConnectionId) -> Result<Connection, EngineError> {
    match engine.dispatch(Command::DuplicateConnection { id }).await? {
        Outcome::Connection { connection } => Ok(connection),
        other => panic!("a connection, got {other:?}"),
    }
}

async fn root_connections(engine: &Engine) -> Vec<Connection> {
    match engine.dispatch(Command::ListConnections).await {
        Ok(Outcome::ConnectionTree { tree }) => tree.connections,
        other => panic!("the tree, got {other:?}"),
    }
}

async fn last_used(engine: &Engine) -> Option<ConnectionId> {
    match engine.dispatch(Command::ListConnections).await {
        Ok(Outcome::ConnectionTree { tree }) => tree.last_used,
        other => panic!("the tree, got {other:?}"),
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

#[tokio::test]
async fn the_copy_has_every_field_and_its_own_secrets_right_under_the_original() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = engine_in(dir.path()).await;
    let source = web(&engine).await;

    let copy = duplicate(&engine, source.id).await.expect("duplicated");

    assert_ne!(copy.id, source.id);
    assert_eq!(copy.name, "web (2)");
    assert_eq!(copy.folder_id, source.folder_id);
    assert_eq!(
        (
            &copy.host,
            copy.port,
            &copy.username,
            copy.auth_method,
            &copy.notes
        ),
        (
            &source.host,
            source.port,
            &source.username,
            source.auth_method,
            &source.notes
        )
    );
    assert_eq!(copy.secrets, source.secrets);
    assert_eq!(copy.key, source.key);
    for kind in SecretKind::ALL {
        assert_eq!(
            secret(&engine, copy.id, kind).await,
            secret(&engine, source.id, kind).await,
            "{kind:?}"
        );
    }
    assert_eq!(
        secret(&engine, copy.id, SecretKind::Password)
            .await
            .as_deref(),
        Some(PASSWORD)
    );
    let names: Vec<String> = root_connections(&engine)
        .await
        .into_iter()
        .map(|connection| connection.name)
        .collect();
    assert_eq!(names, ["alpha", "web", "web (2)", "zulu"]);
}

#[tokio::test]
async fn each_copy_takes_the_next_free_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = engine_in(dir.path()).await;
    let source = web(&engine).await;

    let second = duplicate(&engine, source.id).await.expect("duplicated");
    let third = duplicate(&engine, source.id).await.expect("duplicated");
    let of_copy = duplicate(&engine, second.id).await.expect("duplicated");

    assert_eq!(second.name, "web (2)");
    assert_eq!(third.name, "web (3)");
    assert_eq!(of_copy.name, "web (4)");
}

#[tokio::test]
async fn a_locked_vault_or_an_agent_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = engine_in(dir.path()).await;
    let source = web(&engine).await;
    engine.dispatch(Command::LockVault).await.expect("locked");

    let refused = duplicate(&engine, source.id).await.expect_err("refused");
    assert_eq!(refused.code(), "vault_locked");
    let refused = engine
        .dispatch_mcp(Command::DuplicateConnection { id: source.id })
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "gui_only");

    assert_eq!(root_connections(&engine).await.len(), 3);
}

#[tokio::test]
async fn the_list_names_the_connection_used_last() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = engine_in(dir.path()).await;
    let source = web(&engine).await;
    assert_eq!(last_used(&engine).await, None);

    engine.remember_used(source.id).await;

    assert_eq!(last_used(&engine).await, Some(source.id));
}

#[test]
fn a_trailing_number_is_the_copy_s_own() {
    assert_eq!(strip_number("web"), "web");
    assert_eq!(strip_number("web (2)"), "web");
    assert_eq!(strip_number("web (12)"), "web");
    assert_eq!(strip_number("web (x)"), "web (x)");
    assert_eq!(strip_number("web ()"), "web ()");
    assert_eq!(strip_number("(2)"), "(2)");
}
