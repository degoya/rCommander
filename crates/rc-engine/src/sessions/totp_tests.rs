//! The one-time code of a connection's seed (RC-060-08): made from the vault, refused when the
//! seed is no seed, held back by `connections.totp_autofill`, and kept out of MCP answers.

use rc_db::Db;
use rc_remote::{
    login_script::SecretSource as _,
    ssh::{CredentialError, Credentials as _},
    totp::TotpSeed,
};
use rc_vault::ExposeSecret as _;

use super::super::EngineCredentials;
use crate::test_support;
use crate::{
    Command, ConnectionDraft, ConnectionId, ConnectionPatch, Engine, Outcome,
    SaveConnectionRequest, SaveFields, SecretChange, SecretKind, SecretParam,
    settings::TOTP_AUTOFILL_KEY,
};

/// A seed that is easy to spot: 32 Base32 symbols.
const SEED: &str = "CANARYTOTPSEEDA2CANARYTOTPSEEDB3";

async fn engine_with_vault() -> (tempfile::TempDir, Engine, ConnectionId) {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(dir.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let command = Command::CreateConnection {
        folder_id: None,
        connection: ConnectionDraft::new("web", "web.example.org"),
    };
    let Ok(Outcome::Connection { connection }) = engine.dispatch(command).await else {
        panic!("the connection");
    };
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from("master-password-1"),
        })
        .await
        .expect("create");
    (dir, engine, connection.id)
}

fn credentials(engine: &Engine, connection: ConnectionId) -> EngineCredentials {
    EngineCredentials {
        engine: engine.clone(),
        connection,
        key_path: None,
        announce: false,
    }
}

fn save_seed(id: ConnectionId, seed: &str) -> Command {
    Command::SaveConnection(SaveConnectionRequest {
        fields: SaveFields::Update {
            id,
            patch: ConnectionPatch::default(),
        },
        secrets: vec![SecretChange::Set {
            kind: SecretKind::TotpSeed,
            value: SecretParam::from(seed),
        }],
        login_script: None,
        favourite: None,
        credential: None,
    })
}

#[tokio::test]
async fn a_stored_seed_makes_the_code_of_now() {
    let (_dir, engine, id) = engine_with_vault().await;
    assert!(engine.totp_code(id, false).await.expect("read").is_none());
    let connection = match engine.dispatch(save_seed(id, SEED)).await.expect("saved") {
        Outcome::Connection { connection } => connection,
        other => panic!("the connection, got {other:?}"),
    };
    assert!(connection.secrets.has_totp_seed);
    let before = rc_remote::totp::unix_now();
    let code = credentials(&engine, id)
        .totp_code()
        .await
        .expect("read")
        .expect("a code");
    let after = rc_remote::totp::unix_now();
    let seed = TotpSeed::parse(SEED).expect("valid");
    let expected = [before, after].map(|at| seed.code_at(at).expose_secret().to_owned());
    assert!(expected.contains(&code.expose_secret().to_owned()));
    // The login script's step gets one the same way.
    let script = engine.login_script_secrets(crate::LoginScriptId(1), Some(id));
    assert_eq!(
        script
            .totp_code()
            .await
            .expect("a code")
            .expose_secret()
            .len(),
        6
    );
}

#[tokio::test]
async fn a_seed_that_makes_no_code_is_refused_before_it_is_stored() {
    let (_dir, engine, id) = engine_with_vault().await;
    for broken in [
        "otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP",
        "not a seed!",
        "JBSWY3DP",
    ] {
        let error = engine
            .dispatch(save_seed(id, broken))
            .await
            .expect_err(broken);
        assert_eq!(error.code(), "totp_seed_invalid", "{broken}");
        assert!(!error.to_string().contains(broken), "{error}");
    }
    let Ok(Outcome::Connection { connection }) =
        engine.dispatch(Command::GetConnection { id }).await
    else {
        panic!("the connection");
    };
    assert!(!connection.secrets.has_totp_seed);
}

#[tokio::test]
async fn autofill_off_leaves_the_question_to_the_person() {
    let (_dir, engine, id) = engine_with_vault().await;
    engine.dispatch(save_seed(id, SEED)).await.expect("saved");
    // Protected: only the settings dialog's own path writes it.
    engine
        .set_protected_setting(TOTP_AUTOFILL_KEY, &serde_json::json!(false))
        .await
        .expect("off");
    assert!(
        credentials(&engine, id)
            .totp_code()
            .await
            .expect("read")
            .is_none()
    );
    // An explicit `send_totp` step still sends it.
    let script = engine.login_script_secrets(crate::LoginScriptId(1), Some(id));
    assert!(script.totp_code().await.is_ok());
}

#[tokio::test]
async fn a_locked_vault_is_vault_locked() {
    let (_dir, engine, id) = engine_with_vault().await;
    engine.dispatch(save_seed(id, SEED)).await.expect("saved");
    engine.dispatch(Command::LockVault).await.expect("locked");
    match credentials(&engine, id).totp_code().await {
        Err(CredentialError::Locked) => {}
        other => panic!("locked, got {other:?}"),
    }
}

#[tokio::test]
async fn neither_seed_nor_code_survives_the_mcp_mask() {
    let (_dir, engine, id) = engine_with_vault().await;
    engine.dispatch(save_seed(id, SEED)).await.expect("saved");
    let code = credentials(&engine, id)
        .totp_code()
        .await
        .expect("read")
        .expect("a code");
    let screen = format!(
        "seed {SEED} and code {} on the screen",
        code.expose_secret()
    );
    let scrubbed = engine.scrub_secrets(&screen).expect("scrubbed");
    assert!(!scrubbed.contains(SEED), "{scrubbed}");
    assert!(!scrubbed.contains(code.expose_secret()), "{scrubbed}");
}
