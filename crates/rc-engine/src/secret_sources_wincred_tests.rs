//! The Windows credential manager behind the seam of RC-100-34 (RC-100-37): offered on Windows
//! while its setting is on, elsewhere neither offered nor asked; and an entry a ready source
//! lacks is named as such, not as a broken source.

use rc_db::{Db, connections::ConnectionDraft};
use rc_plugin_host::testbed::{
    err_failure,
    secrets::{Export, SecretsBed},
};
use serde_json::json;

use super::{tests::Setup, *};
use crate::{Command, ConnectionId, Outcome, SecretRef, test_support};

/// A ready source whose `get` answers `not_found`: the entry is gone, not the source broken.
#[tokio::test]
async fn an_entry_the_source_lacks_is_named() {
    let mut bed = SecretsBed::new();
    let code = bed.data(b"tests.gone");
    let setup = Setup::new(&bed.body(Export::Get, err_failure(0, code))).await;
    setup.map().await;
    let (code, message) = setup.failure().await;
    assert_eq!(code, "secret_source_entry_missing");
    assert!(message.contains("Test source"), "{message}");
}

/// An engine with a database file, the credential manager's setting `on`, and a connection.
async fn engine(on: bool) -> (tempfile::TempDir, Engine, Db, ConnectionId) {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(root.path()).await;
    let db = Db::open(root.path().join("rcommander.db"))
        .await
        .expect("db");
    assert!(engine.attach_db(db.clone()));
    engine
        .set_protected_setting(WINCRED_SOURCE_KEY, &json!(on))
        .await
        .expect("set");
    let connection = db
        .create_connection(None, &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("create")
        .id;
    (root, engine, db, connection)
}

fn reference(connection: ConnectionId) -> SecretSourceRef {
    SecretSourceRef {
        connection_id: connection,
        kind: SecretKind::Password,
        source: WINCRED_SOURCE.to_owned(),
        entry_id: "git:https://example.org".to_owned(),
        entry_label: "example.org".to_owned(),
    }
}

async fn listed(engine: &Engine) -> Vec<SecretSourceInfo> {
    let listed = engine
        .dispatch(Command::ListSecretSources {
            connection_id: None,
        })
        .await
        .expect("listed");
    let Outcome::SecretSources(outcome) = listed else {
        panic!("sources, got {listed:?}");
    };
    let SecretSourcesOutcome::Sources { sources, .. } = *outcome else {
        panic!("sources");
    };
    sources
}

/// What a source that is not offered answers: a connect, a search and a mapping all name it
/// missing, and nothing asks the credential manager.
async fn assert_not_offered(engine: &Engine, db: &Db, connection: ConnectionId) {
    assert!(
        listed(engine)
            .await
            .iter()
            .all(|source| source.source != WINCRED_SOURCE)
    );
    let refused = engine
        .dispatch(Command::MapSecretSource(reference(connection)))
        .await
        .expect_err("not offered");
    assert_eq!(refused.code(), "secret_source_missing");
    let refused = engine
        .dispatch(Command::SearchSecretSource {
            source: WINCRED_SOURCE.to_owned(),
            query: String::new(),
        })
        .await
        .expect_err("not offered");
    assert_eq!(refused.code(), "secret_source_missing");
    // A reference that came with an import.
    db.set_secret_source(&reference(connection))
        .await
        .expect("stored");
    let refused = engine
        .resolve_secret(SecretRef {
            connection,
            kind: SecretKind::Password,
        })
        .await
        .expect_err("not offered");
    assert_eq!(refused.code(), "secret_source_missing");
}

#[cfg(not(windows))]
#[tokio::test]
async fn elsewhere_the_credential_manager_is_neither_offered_nor_asked() {
    let (_root, engine, db, connection) = engine(true).await;
    assert_not_offered(&engine, &db, connection).await;
}

#[tokio::test]
async fn switched_off_the_credential_manager_is_not_offered() {
    let (_root, engine, db, connection) = engine(false).await;
    assert_not_offered(&engine, &db, connection).await;
}

/// On Windows with the setting on it is listed ready and can be mapped.
#[cfg(windows)]
#[tokio::test]
async fn on_windows_the_credential_manager_is_offered() {
    let (_root, engine, _db, connection) = engine(true).await;
    let sources = listed(&engine).await;
    let wincred = sources
        .iter()
        .find(|source| source.source == WINCRED_SOURCE)
        .expect("offered");
    assert_eq!(wincred.state, "ready");
    engine
        .dispatch(Command::MapSecretSource(reference(connection)))
        .await
        .expect("mapped");
}
