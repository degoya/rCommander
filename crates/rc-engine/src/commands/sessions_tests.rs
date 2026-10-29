//! The session commands against the in-process SSH server: connect, the vault, host keys,
//! settings, test and disconnect. The pool's channels, loss and idle close are in
//! `sessions_pool_tests.rs`.

use std::time::{Duration, Instant};

use rc_db::Db;
use rc_remote::{
    OfferedKey,
    testkit::{
        PASSWORD, ServerOptions, TestServer, fingerprint, host_key_a, host_key_b, silent_server,
    },
    trust,
};
use tokio::sync::broadcast;

use crate::test_support;
use crate::{
    AuthKind, AuthUsed, Command, ConnectionDraft, ConnectionId, Engine, EngineEvent, Outcome,
    SecretKind, SecretParam, SessionChange, SessionPrefs, SshState,
};

pub(super) struct Harness {
    _root: tempfile::TempDir,
    pub engine: Engine,
    pub db: Db,
    pub server: TestServer,
    pub id: ConnectionId,
}

/// An engine with an unlocked vault, no SSH agent, and connection `id` to a test server that
/// takes [`PASSWORD`], stored in the vault; the server's key is trusted when `trusted`.
pub(super) async fn harness(prefs: Option<SessionPrefs>, trusted: bool) -> Harness {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let engine = test_support::headless(root.path()).await;
    engine.attach_db(db.clone());
    engine.attach_agent_sources(Vec::new());
    let vault = test_support::fast_vault(db.clone()).await;
    engine.attach_vault(vault);
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from("correct horse battery"),
        })
        .await
        .expect("vault created");
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let id = connection_to(&engine, server.port).await;
    engine
        .dispatch(test_support::store_secret(
            id,
            SecretKind::Password,
            SecretParam::from(PASSWORD),
        ))
        .await
        .expect("password stored");
    if let Some(prefs) = prefs {
        engine
            .dispatch(Command::SetSessionPrefs { id, prefs })
            .await
            .expect("settings stored");
    }
    if trusted {
        let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
        trust::trust(&db, "127.0.0.1", server.port, offered)
            .await
            .expect("trusted");
    }
    Harness {
        _root: root,
        engine,
        db,
        server,
        id,
    }
}

pub(super) async fn connection_to(engine: &Engine, port: u16) -> ConnectionId {
    let draft = ConnectionDraft {
        port: Some(i64::from(port)),
        username: Some("tester".to_owned()),
        ..ConnectionDraft::new(format!("box {port}"), "127.0.0.1")
    };
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    connection.id
}

/// The events sent so far.
pub(super) fn drain(events: &mut broadcast::Receiver<EngineEvent>) -> Vec<EngineEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

fn code(result: Result<Outcome, crate::EngineError>) -> &'static str {
    result.expect_err("an error").code()
}

#[tokio::test]
async fn connect_logs_in_with_the_stored_password_and_announces_it() {
    let harness = harness(None, true).await;
    let mut events = harness.engine.subscribe();
    let outcome = harness
        .engine
        .dispatch(Command::ConnectSession { id: harness.id })
        .await
        .expect("connected");
    let Outcome::Session(status) = outcome else {
        panic!("a session: {outcome:?}");
    };
    assert_eq!(status.state, SshState::Connected);
    assert_eq!(status.login, Some(AuthUsed::Password));
    assert_eq!(
        status.host_key_fingerprint,
        Some(fingerprint(&host_key_a()))
    );
    assert!(drain(&mut events).contains(&EngineEvent::Session {
        connection: harness.id,
        change: SessionChange::Connected,
    }));
    // A second connect takes the session that is up.
    harness
        .engine
        .dispatch(Command::ConnectSession { id: harness.id })
        .await
        .expect("connected");
    assert_eq!(harness.server.accepts(), 1);
    assert_eq!(harness.engine.ssh_sessions(), vec![status]);
}

#[tokio::test]
async fn mcp_with_a_locked_vault_answers_vault_locked_and_asks_no_one() {
    let harness = harness(None, true).await;
    harness
        .engine
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let mut events = harness.engine.subscribe();
    let error = harness
        .engine
        .dispatch_mcp(Command::ConnectSession { id: harness.id })
        .await;
    assert_eq!(code(error), "vault_locked");
    let test = harness
        .engine
        .dispatch_mcp(Command::TestConnection { id: harness.id })
        .await;
    assert_eq!(code(test), "vault_locked");
    assert!(
        !drain(&mut events)
            .iter()
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "no unlock dialog for an agent's call"
    );
    // The window's connect asks the person instead.
    let error = harness
        .engine
        .dispatch(Command::ConnectSession { id: harness.id })
        .await;
    assert_eq!(code(error), "vault_locked");
    assert!(
        drain(&mut events)
            .iter()
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. }))
    );
}

#[tokio::test]
async fn an_unknown_host_key_is_a_question_for_a_person_and_never_trusted() {
    let harness = harness(None, false).await;
    let mut events = harness.engine.subscribe();
    let error = harness
        .engine
        .dispatch_mcp(Command::ConnectSession { id: harness.id })
        .await;
    assert_eq!(code(error), "host_key_unknown");
    assert!(
        drain(&mut events)
            .iter()
            .any(|event| matches!(event, EngineEvent::HostKeyQuestion { .. }))
    );
    let stored = harness
        .db
        .known_host_candidates("127.0.0.1", harness.server.port)
        .await
        .expect("read");
    assert!(stored.is_empty(), "nothing was stored");
    assert_eq!(
        harness.server.auth_attempts(),
        0,
        "no secret before a trusted key"
    );
}

#[tokio::test]
async fn a_changed_host_key_stops_before_any_secret_is_sent() {
    let harness = harness(None, false).await;
    let impostor = OfferedKey::from_public_key(host_key_b().public_key()).expect("a key");
    trust::trust(&harness.db, "127.0.0.1", harness.server.port, impostor)
        .await
        .expect("trusted");
    let mut events = harness.engine.subscribe();
    let error = harness
        .engine
        .dispatch(Command::ConnectSession { id: harness.id })
        .await;
    assert_eq!(code(error), "host_key_changed");
    assert_eq!(harness.server.auth_attempts(), 0);
    assert!(drain(&mut events).iter().any(|event| matches!(
        event,
        EngineEvent::HostKeyQuestion { question }
            if matches!(question.kind, crate::HostKeyQuestionKind::Changed { .. })
    )));
}

#[tokio::test]
async fn test_reports_the_method_and_leaves_no_session() {
    let harness = harness(None, true).await;
    let outcome = harness
        .engine
        .dispatch_mcp(Command::TestConnection { id: harness.id })
        .await
        .expect("tested");
    let Outcome::SessionTested(report) = outcome else {
        panic!("a report: {outcome:?}");
    };
    assert_eq!(report.login, AuthUsed::Password);
    assert_eq!(report.host_key.fingerprint, fingerprint(&host_key_a()));
    assert!(report.duration_ms < 10_000);
    assert_eq!(harness.engine.ssh_session(harness.id), None);
}

#[tokio::test]
async fn settings_are_read_back_and_checked() {
    let prefs = SessionPrefs {
        auth_order: vec![AuthKind::Password],
        keepalive_secs: 10,
        idle_close_secs: 0,
        agent_fingerprint: None,
    };
    let harness = harness(Some(prefs.clone()), true).await;
    let read = harness
        .engine
        .dispatch_mcp(Command::GetSessionPrefs { id: harness.id })
        .await
        .expect("read");
    assert_eq!(
        read,
        Outcome::SessionPrefs {
            connection_id: harness.id,
            prefs,
        }
    );
    let invalid = harness
        .engine
        .dispatch_mcp(Command::SetSessionPrefs {
            id: harness.id,
            prefs: SessionPrefs {
                auth_order: Vec::new(),
                ..SessionPrefs::default()
            },
        })
        .await;
    assert_eq!(code(invalid), "session_prefs_invalid");
    let missing = harness
        .engine
        .dispatch(Command::GetSessionPrefs {
            id: ConnectionId(999),
        })
        .await;
    assert_eq!(code(missing), "connection_not_found");
}

#[tokio::test]
async fn disconnect_cancels_a_hanging_setup_within_a_second() {
    let harness = harness(None, true).await;
    let (port, _silent) = silent_server().await;
    let id = connection_to(&harness.engine, port).await;
    let engine = harness.engine.clone();
    let setup = tokio::spawn(async move { engine.dispatch(Command::ConnectSession { id }).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness.engine.ssh_session(id).map(|status| status.state),
        Some(SshState::Connecting)
    );
    let started = Instant::now();
    let closed = harness
        .engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("disconnected");
    assert_eq!(
        closed,
        Outcome::SessionClosed {
            connection_id: id,
            was_open: true,
        }
    );
    let result = setup.await.expect("joined");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(code(result), "cancelled");
    assert_eq!(harness.engine.ssh_session(id), None);
}

#[tokio::test]
async fn disconnect_closes_the_session() {
    let harness = harness(None, true).await;
    harness
        .engine
        .dispatch(Command::ConnectSession { id: harness.id })
        .await
        .expect("connected");
    let mut events = harness.engine.subscribe();
    let closed = harness
        .engine
        .dispatch(Command::DisconnectSession { id: harness.id })
        .await
        .expect("disconnected");
    assert_eq!(
        closed,
        Outcome::SessionClosed {
            connection_id: harness.id,
            was_open: true,
        }
    );
    assert_eq!(harness.engine.ssh_session(harness.id), None);
    let change = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(EngineEvent::Session { change, .. }) = events.recv().await {
                return change;
            }
        }
    })
    .await
    .expect("announced");
    assert_eq!(change, SessionChange::Closed);
}
