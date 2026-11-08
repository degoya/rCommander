//! Sessions through jump hosts (RC-110-03): three in-process servers on loopback, each hop a
//! connection of its own with its own secrets and trust.

use std::time::{Duration, Instant};

use rc_remote::testkit::{CLIENT_ED25519, ServerOptions, TestServer, host_key_a, host_key_b};

use super::super::jump_test_support::Bench;
use crate::{
    AuthMethod, Command, ConnectionId, ConnectionPatch, EngineError, EngineEvent, Outcome,
    SessionChange, SshExecRequest, SshState, test_support,
};

/// Runs `command` on connection `id` (`ssh.exec`) and answers what it printed.
async fn run(bench: &Bench, id: ConnectionId, command: &str) -> String {
    let request = SshExecRequest {
        connection_id: id,
        command: command.to_owned(),
        stdin: None,
        timeout_secs: None,
        max_output_bytes: None,
    };
    match bench.engine.dispatch(Command::SshExec(request)).await {
        Ok(Outcome::SshExec(output)) => output.stdout,
        other => panic!("ran: {other:?}"),
    }
}

#[tokio::test]
async fn a_session_runs_through_two_jumps() {
    let bench = Bench::new().await;
    let bastion = Bench::server(true).await;
    let inner = Bench::server(true).await;
    let goal = Bench::server(false).await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    let second = bench
        .connection("inner", inner.port, Some(first), true)
        .await;
    let target = bench
        .connection("target", goal.port, Some(second), true)
        .await;
    let started = Instant::now();
    let Ok(Outcome::Session(status)) = bench
        .engine
        .dispatch(Command::ConnectSession { id: target })
        .await
    else {
        panic!("connected");
    };
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(status.state, SshState::Connected);
    assert_eq!(status.via, ["bastion", "inner"]);
    assert_eq!(run(&bench, target, "whoami").await, "ran: whoami");
    for server in [&bastion, &inner, &goal] {
        assert_eq!(server.accepts(), 1, "one TCP connection each");
    }
    // The jump hosts' sessions are the chain's, not the pool's.
    let pooled: Vec<_> = bench
        .engine
        .ssh_sessions()
        .into_iter()
        .map(|status| status.connection_id)
        .collect();
    assert_eq!(pooled, [target]);
    let Ok(Outcome::SessionTested(report)) = bench
        .engine
        .dispatch_mcp(Command::TestConnection { id: target })
        .await
    else {
        panic!("tested");
    };
    assert_eq!(report.hops, ["bastion", "inner"]);
}

#[tokio::test]
async fn a_changed_hop_key_stops_the_chain_before_any_secret() {
    let bench = Bench::new().await;
    let bastion = Bench::server(true).await;
    let impostor = TestServer::start(ServerOptions {
        direct_tcpip: true,
        ..ServerOptions::new(host_key_b())
    })
    .await;
    let goal = Bench::server(false).await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    // Key a is stored for the second hop; it presents key b.
    let second = bench
        .connection("inner", impostor.port, Some(first), true)
        .await;
    let target = bench
        .connection("target", goal.port, Some(second), true)
        .await;
    let mut events = bench.engine.subscribe();
    let error = bench
        .engine
        .dispatch_mcp(Command::ConnectSession { id: target })
        .await
        .expect_err("stopped");
    assert_eq!(error.code(), "host_key_changed");
    let EngineError::Hop {
        hop, index, total, ..
    } = &error
    else {
        panic!("named by its hop: {error:?}");
    };
    assert_eq!((hop.as_str(), *index, *total), ("inner", 2, 3));
    assert_eq!(impostor.auth_attempts(), 0, "no secret to the changed hop");
    assert_eq!(goal.accepts(), 0, "nothing reached the target");
    let question = std::iter::from_fn(|| events.try_recv().ok()).find_map(|event| match event {
        EngineEvent::HostKeyQuestion { question } => Some(question),
        _ => None,
    });
    let hop = question
        .and_then(|question| question.hop)
        .expect("labelled");
    assert_eq!((hop.index, hop.total, hop.name.as_str()), (2, 3, "inner"));
}

#[tokio::test]
async fn an_unknown_key_on_the_second_hop_is_a_question_and_never_trusted_over_mcp() {
    let bench = Bench::new().await;
    let bastion = Bench::server(true).await;
    let inner = Bench::server(true).await;
    let goal = Bench::server(false).await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    let second = bench
        .connection("inner", inner.port, Some(first), false)
        .await;
    let target = bench
        .connection("target", goal.port, Some(second), true)
        .await;
    let error = bench
        .engine
        .dispatch_mcp(Command::ConnectSession { id: target })
        .await
        .expect_err("a question");
    assert_eq!(error.code(), "host_key_unknown");
    let stored = bench
        .db
        .known_host_candidates("127.0.0.1", inner.port)
        .await
        .expect("read");
    assert!(stored.is_empty(), "nothing was trusted");
    assert_eq!(inner.auth_attempts(), 0);
}

#[tokio::test]
async fn each_hop_logs_in_with_its_own_secrets() {
    let bench = Bench::new().await;
    // The jump host takes only its password, the target only the client keys.
    let bastion = TestServer::start(ServerOptions {
        direct_tcpip: true,
        authorized: Vec::new(),
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let goal = TestServer::start(ServerOptions {
        password: None,
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    let target = bench
        .connection("target", goal.port, Some(first), true)
        .await;
    let key_file = tempfile::NamedTempFile::new().expect("key file");
    std::fs::write(key_file.path(), CLIENT_ED25519).expect("written");
    let patch = ConnectionPatch {
        auth_method: Some(AuthMethod::Key),
        key_path: Some(key_file.path().display().to_string()),
        ..ConnectionPatch::default()
    };
    bench
        .engine
        .dispatch(Command::UpdateConnection { id: target, patch })
        .await
        .expect("a key file");
    bench
        .engine
        .dispatch(test_support::clear_secret(
            target,
            crate::SecretKind::Password,
        ))
        .await
        .expect("no password at the target");
    bench
        .engine
        .dispatch(Command::ConnectSession { id: target })
        .await
        .expect("both hops logged in");
    // A locked vault holds the jump host's password back: an agent's call asks no one.
    bench
        .engine
        .dispatch(Command::DisconnectSession { id: target })
        .await
        .expect("closed");
    bench
        .engine
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let mut events = bench.engine.subscribe();
    let error = bench
        .engine
        .dispatch_mcp(Command::ConnectSession { id: target })
        .await
        .expect_err("locked");
    assert_eq!(error.code(), "vault_locked");
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "no unlock dialog for an agent's call"
    );
}

#[tokio::test]
async fn a_lost_jump_host_rebuilds_the_chain_at_the_next_use() {
    let bench = Bench::new().await;
    let bastion = Bench::server(true).await;
    let goal = Bench::server(false).await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    for id in [first] {
        bench
            .engine
            .dispatch(Command::SetSessionPrefs {
                id,
                prefs: crate::SessionPrefs {
                    keepalive_secs: 1,
                    ..crate::SessionPrefs::default()
                },
            })
            .await
            .expect("keepalive");
    }
    let target = bench
        .connection("target", goal.port, Some(first), true)
        .await;
    let mut events = bench.engine.subscribe();
    bench
        .engine
        .dispatch(Command::ConnectSession { id: target })
        .await
        .expect("connected");
    bastion.freeze();
    let limit = Duration::from_secs(u64::try_from(rc_remote::ssh::KEEPALIVE_MAX).expect("small"))
        + Duration::from_secs(5);
    tokio::time::timeout(limit, async {
        loop {
            match events.recv().await {
                Ok(EngineEvent::Session {
                    connection,
                    change: SessionChange::Lost,
                }) if connection == target => return,
                Ok(_) => {}
                Err(error) => panic!("events: {error}"),
            }
        }
    })
    .await
    .expect("the target noticed the loss");
    assert_eq!(run(&bench, target, "again").await, "ran: again");
    assert_eq!(bastion.accepts(), 2, "the chain was built anew");
    assert_eq!(goal.accepts(), 2);
}

#[tokio::test]
async fn a_jump_host_named_over_mcp_is_an_endpoint_change() {
    let bench = Bench::new().await;
    let bastion = Bench::server(true).await;
    let goal = Bench::server(false).await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    let target = bench.connection("target", goal.port, None, true).await;
    let error = bench
        .engine
        .dispatch_mcp(Command::UpdateConnection {
            id: target,
            patch: ConnectionPatch {
                jump_id: Some(first.0),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect_err("a password is stored");
    assert_eq!(error.code(), "endpoint_change_needs_gui");
    let Ok(Outcome::Connection { connection }) = bench
        .engine
        .dispatch(Command::UpdateConnection {
            id: target,
            patch: ConnectionPatch {
                jump_id: Some(first.0),
                ..ConnectionPatch::default()
            },
        })
        .await
    else {
        panic!("the person in the window sets it");
    };
    assert_eq!(connection.jump_id, Some(first));
}

#[tokio::test]
async fn an_sftp_listing_of_the_target_runs_through_two_jumps() {
    let bench = Bench::new().await;
    let bastion = Bench::server(true).await;
    let inner = Bench::server(true).await;
    let served = tempfile::tempdir().expect("served");
    std::fs::create_dir_all(served.path().join("home/tester/docs")).expect("home");
    std::fs::write(served.path().join("home/tester/readme.txt"), b"hello").expect("file");
    let mut serve = rc_remote::testkit::SftpServe::new(served.path());
    serve.home = "/home/tester".to_owned();
    let goal = TestServer::start(ServerOptions {
        sftp: Some(serve),
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let first = bench.connection("bastion", bastion.port, None, true).await;
    let second = bench
        .connection("inner", inner.port, Some(first), true)
        .await;
    bench
        .connection("target", goal.port, Some(second), true)
        .await;
    let listing = bench
        .engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("sftp://target".to_owned()),
        })
        .await
        .expect("the target's home through both jump hosts");
    let Outcome::Listing { entries, .. } = listing else {
        panic!("a listing");
    };
    let names: Vec<String> = entries.into_iter().map(|entry| entry.name).collect();
    assert_eq!(names, ["docs", "readme.txt"]);
    for server in [&bastion, &inner, &goal] {
        assert_eq!(server.accepts(), 1, "one TCP connection each");
    }
    assert_eq!(bastion.direct_opens(), 1);
    assert_eq!(inner.direct_opens(), 1);
}
