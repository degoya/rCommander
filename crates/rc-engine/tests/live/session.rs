//! "Test", connect and disconnect; the unknown host key that is asked about before any password
//! goes out; the changed key that stops the connection and is replaced only by its own command;
//! a locked vault that asks for the master password and nothing else.

use rc_engine::{
    Command, EngineEvent, HostKeyAnswer, HostKeyQuestionKind, Outcome, SecretParam, SshState,
};

use crate::live::{MASTER, connect_trusting, connection, engine, trust_open_question, workdir};
use crate::live_or_skip;

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn test_connect_and_disconnect_a_saved_connection() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;

    let untrusted = engine
        .dispatch(Command::TestConnection { id })
        .await
        .expect_err("the key is asked about first");
    assert_eq!(untrusted.code(), "host_key_unknown", "{untrusted}");
    trust_open_question(&engine).await;

    let tested = engine.dispatch(Command::TestConnection { id }).await;
    let Ok(Outcome::SessionTested(report)) = tested else {
        panic!("a test report, got {tested:?}");
    };
    assert_eq!(report.connection_id, id);
    assert!(
        live.fingerprints("a")
            .contains(&report.host_key.fingerprint),
        "{} is one of the server's keys",
        report.host_key.fingerprint
    );
    eprintln!("test: {:?} in {} ms", report.login, report.duration_ms);

    let connected = engine.dispatch(Command::ConnectSession { id }).await;
    let Ok(Outcome::Session(status)) = connected else {
        panic!("a session, got {connected:?}");
    };
    assert_eq!(status.state, SshState::Connected);
    let closed = engine.dispatch(Command::DisconnectSession { id }).await;
    assert!(
        matches!(closed, Ok(Outcome::SessionClosed { was_open: true, .. })),
        "{closed:?}"
    );
}

/// The `keys` server alone: its log counts login attempts, and it gets a new key mid-test.
#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn an_unknown_key_sends_no_password_and_a_changed_key_is_replaced_only_on_purpose() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-keys", "keys").await;

    let before = live.userauth_requests("keys");
    let unknown = engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect_err("unknown");
    assert_eq!(unknown.code(), "host_key_unknown", "{unknown}");
    let question = engine.host_key_questions().pop().expect("a question");
    assert_eq!(question.kind, HostKeyQuestionKind::Unknown);
    let first_key = question.offered.fingerprint.clone();
    assert!(live.fingerprints("keys").contains(&first_key));
    assert!(unknown.to_string().contains(&first_key), "{unknown}");
    assert_eq!(
        live.userauth_requests("keys"),
        before,
        "no login attempt before the key is trusted"
    );

    trust_open_question(&engine).await;
    engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect("connected once trusted");
    assert!(live.userauth_requests("keys") > before, "now it logged in");
    engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("closed");

    live.ctl(&["rekey", "keys"]);
    let before = live.userauth_requests("keys");
    let changed = engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect_err("a changed key stops the connection");
    assert_eq!(changed.code(), "host_key_changed", "{changed}");
    let question = engine
        .host_key_questions()
        .into_iter()
        .find(|question| matches!(question.kind, HostKeyQuestionKind::Changed { .. }))
        .expect("a question about the changed key");
    let new_key = question.offered.fingerprint.clone();
    assert_ne!(new_key, first_key);
    assert!(live.fingerprints("keys").contains(&new_key));
    let message = changed.to_string();
    assert!(
        message.contains(&new_key) && message.contains(&first_key),
        "both fingerprints: {message}"
    );
    assert_eq!(live.userauth_requests("keys"), before, "no login attempt");

    for answer in [HostKeyAnswer::TrustAndSave, HostKeyAnswer::ConnectOnce] {
        let refused = engine
            .dispatch(Command::AnswerHostKey {
                question: question.id,
                answer,
            })
            .await;
        assert!(refused.is_err(), "{answer:?} does not pass a changed key");
    }
    let still = engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect_err("still stopped");
    assert_eq!(still.code(), "host_key_changed");

    let replaced = engine
        .dispatch(Command::ReplaceHostKey {
            question: question.id,
        })
        .await;
    let Ok(Outcome::HostKeyReplaced { key }) = replaced else {
        panic!("replaced, got {replaced:?}");
    };
    assert_eq!(key.fingerprint_sha256, new_key);
    engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect("connected with the new key");
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_locked_vault_asks_for_the_master_password_and_nothing_else() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("closed");
    engine.dispatch(Command::LockVault).await.expect("locked");

    let mut events = engine.subscribe();
    let refused = engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect_err("locked");
    assert_eq!(refused.code(), "vault_locked", "{refused}");
    let seen: Vec<EngineEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        seen.iter()
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "the window is asked for the master password: {seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, EngineEvent::HostKeyQuestion { .. })),
        "{seen:?}"
    );
    assert!(engine.host_key_questions().is_empty());

    engine
        .dispatch(Command::UnlockVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("unlocked");
    let listing = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("sftp://live-a/home/tester/docs".to_owned()),
        })
        .await;
    assert!(
        matches!(listing, Ok(Outcome::Listing { .. })),
        "{listing:?}"
    );
}
