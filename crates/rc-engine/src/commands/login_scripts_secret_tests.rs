use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, duplex};
use tokio_util::sync::CancellationToken;

use super::tests::{MASTER, connection, create, engine_in, sudo_steps};
use crate::{
    Command, Engine, EngineEvent, LoginScriptInfo, LoginScriptPatch, Outcome, REDACTED,
    ScriptSecretRef, SecretError, SecretParam, SecretSource as _, login_script,
};

const CANARY: &str = "hunter2-root-canary";

async fn store(engine: &Engine, script: &LoginScriptInfo) -> Result<i64, crate::EngineError> {
    let outcome = engine
        .dispatch(Command::SetLoginScriptSecret {
            script_id: script.id,
            label: "root".into(),
            value: SecretParam::from(CANARY),
        })
        .await?;
    let Outcome::LoginScript { script } = outcome else {
        panic!("a script, got {outcome:?}");
    };
    Ok(script.secrets[0].id)
}

async fn create_vault(engine: &Engine) {
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault");
}

#[tokio::test]
async fn a_script_secret_needs_a_vault_and_is_stored_only_encrypted() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let script = create(&engine, "su").await;
    let absent = store(&engine, &script).await.expect_err("no vault yet");
    assert_eq!(absent.code(), "vault_absent");

    create_vault(&engine).await;
    let id = store(&engine, &script).await.expect("stored");
    let Ok(Outcome::LoginScript { script }) = engine
        .dispatch(Command::UpdateLoginScript {
            id: script.id,
            patch: LoginScriptPatch {
                steps: Some(sudo_steps(Some(id))),
                ..LoginScriptPatch::default()
            },
        })
        .await
    else {
        panic!("an updated script");
    };
    assert_eq!(script.secrets[0].label, "root");
    let answer = serde_json::to_string(&script).expect("serializes");
    assert!(!answer.contains(CANARY), "{answer}");

    // Neither the steps' JSON nor any other byte of the database holds the value.
    let db = engine.db().expect("db").clone();
    assert!(
        !db.login_script(script.id)
            .await
            .expect("row")
            .steps
            .contains(CANARY)
    );
    for file in ["rcommander.db", "rcommander.db-wal"] {
        let bytes = std::fs::read(dir.path().join(file)).unwrap_or_default();
        let found = bytes.windows(CANARY.len()).any(|w| w == CANARY.as_bytes());
        assert!(!found, "{file} holds the secret in clear");
    }
}

#[tokio::test]
async fn a_run_reads_the_secret_sends_it_and_the_mask_scrubs_its_echo() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    create_vault(&engine).await;
    let script = create(&engine, "su").await;
    let id = store(&engine, &script).await.expect("stored");
    let web = connection(&engine).await;
    // The vault locks and opens again: the run reads the value from the database.
    engine.dispatch(Command::LockVault).await.expect("lock");
    engine
        .dispatch(Command::UnlockVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("unlock");

    let run_script = login_script::LoginScript {
        steps: sudo_steps(Some(id)),
        ..login_script::LoginScript::default()
    };
    let secrets = engine.login_script_secrets(script.id, Some(web));
    let (mut ours, mut server) = duplex(4096);
    let run = async {
        login_script::run_script(&run_script, &mut ours, &secrets, &CancellationToken::new()).await
    };
    let host = async {
        let mut seen = Vec::new();
        let mut byte = [0_u8; 1];
        while !seen.ends_with(b"sudo -i\r") {
            server.read_exact(&mut byte).await.expect("read");
            seen.push(byte[0]);
        }
        server.write_all(b"Password: ").await.expect("write");
        let mut typed = vec![0_u8; CANARY.len() + 1];
        server.read_exact(&mut typed).await.expect("read");
        typed
    };
    let (outcome, typed) = tokio::join!(run, host);
    assert_eq!(
        outcome.expect("runs"),
        login_script::ScriptOutcome::Completed
    );
    assert_eq!(typed, format!("{CANARY}\r").into_bytes());

    let screen = format!("Password: {CANARY}\r\nroot# ");
    let scrubbed = engine.scrub_secrets(&screen).expect("scrubbed");
    assert!(
        !scrubbed.contains(CANARY) && scrubbed.contains(REDACTED),
        "{scrubbed}"
    );
}

#[tokio::test]
async fn a_locked_vault_stops_a_run_before_anything_is_sent() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    create_vault(&engine).await;
    let script = create(&engine, "su").await;
    let id = store(&engine, &script).await.expect("stored");
    engine.dispatch(Command::LockVault).await.expect("lock");

    let secrets = engine.login_script_secrets(script.id, None);
    assert_eq!(
        secrets.secret(ScriptSecretRef::Stored { id }).await.err(),
        Some(SecretError::Locked)
    );
    assert_eq!(
        secrets
            .secret(ScriptSecretRef::ConnectionPassword)
            .await
            .err(),
        Some(SecretError::Missing),
        "no connection, no connection password"
    );
    assert_eq!(
        secrets
            .secret(ScriptSecretRef::Stored { id: id + 50 })
            .await
            .err(),
        Some(SecretError::Missing)
    );

    let run_script = login_script::LoginScript {
        steps: sudo_steps(Some(id)),
        ..login_script::LoginScript::default()
    };
    let (mut ours, mut server) = duplex(4096);
    let error =
        login_script::run_script(&run_script, &mut ours, &secrets, &CancellationToken::new())
            .await
            .expect_err("refused");
    assert_eq!(error.code(), "vault_locked");
    drop(ours);
    let mut sent = Vec::new();
    server.read_to_end(&mut sent).await.expect("read");
    assert!(sent.is_empty(), "{sent:?}");
}

#[tokio::test]
async fn removing_the_last_script_secret_offers_to_remove_the_vault() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    create_vault(&engine).await;
    let script = create(&engine, "su").await;
    let id = store(&engine, &script).await.expect("stored");
    let mut events = engine.subscribe();
    let Ok(Outcome::LoginScript { script }) = engine
        .dispatch(Command::ClearLoginScriptSecret {
            script_id: script.id,
            secret_id: id,
        })
        .await
    else {
        panic!("the script");
    };
    assert!(script.secrets.is_empty());
    let seen: Vec<EngineEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(seen.contains(&EngineEvent::VaultUnused), "{seen:?}");
    let again = engine
        .dispatch(Command::ClearLoginScriptSecret {
            script_id: script.id,
            secret_id: id,
        })
        .await
        .expect_err("gone");
    assert_eq!(again.code(), "login_script_secret_not_found");
}
