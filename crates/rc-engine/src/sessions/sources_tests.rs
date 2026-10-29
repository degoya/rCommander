//! A connection's key at connect time (RC-050-06): a key path on a share is refused unread
//! with `key_path_refused`; a key stored in the vault is used first, whatever the path says.

use rc_db::Db;
use rc_remote::ssh::{CredentialError, Credentials as _};
use rc_vault::ExposeSecret as _;

use super::EngineCredentials;
use crate::test_support;
use crate::{AuthMethod, Command, ConnectionDraft, ConnectionId, Engine, Outcome, SecretParam};

const SHARE: &str = r"\\server\share\k";
const KEY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-remote/tests/fixtures/client_key_ed25519"
);

async fn engine_with_connection() -> (tempfile::TempDir, Engine, ConnectionId) {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(dir.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Key,
        key_path: Some(SHARE.to_owned()),
        ..ConnectionDraft::new("web", "web.example.org")
    };
    let command = Command::CreateConnection {
        folder_id: None,
        connection: draft,
    };
    let Ok(Outcome::Connection { connection }) = engine.dispatch(command).await else {
        panic!("the connection");
    };
    (dir, engine, connection.id)
}

fn credentials(engine: &Engine, connection: ConnectionId) -> EngineCredentials {
    EngineCredentials {
        engine: engine.clone(),
        connection,
        key_path: Some(SHARE.to_owned()),
        announce: false,
    }
}

#[tokio::test]
async fn a_key_path_on_a_share_is_refused_unread() {
    let (_dir, engine, id) = engine_with_connection().await;
    match credentials(&engine, id).private_key().await {
        Err(CredentialError::Failed { code, .. }) => assert_eq!(code, "key_path_refused"),
        other => panic!("refused, got {other:?}"),
    }
}

#[tokio::test]
async fn a_key_in_the_vault_comes_first() {
    let (_dir, engine, id) = engine_with_connection().await;
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from("master-password-1"),
        })
        .await
        .expect("create");
    engine
        .dispatch(Command::ImportKey(crate::ImportKeyRequest {
            connection_id: id,
            path: KEY.to_owned(),
            passphrase: None,
        }))
        .await
        .expect("taken in");
    let material = credentials(&engine, id)
        .private_key()
        .await
        .expect("the stored key")
        .expect("a key");
    assert_eq!(
        material.key.expose_secret(),
        std::fs::read_to_string(KEY).expect("the key file")
    );
}
