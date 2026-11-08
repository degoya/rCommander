//! Sessions through jump hosts (RC-110-03) and `direct-tcpip` channels, against three in-process
//! servers on loopback.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use russh::ChannelMsg;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    HOST_KEY_CHANGED,
    ssh::SshTarget,
    testkit::{
        MemoryCredentials, MemoryGate, PASSWORD, ServerOptions, TestServer, echo_server,
        host_key_a, host_key_b, silent_server,
    },
};

fn target(port: u16) -> SshTarget {
    SshTarget {
        host: "127.0.0.1".to_owned(),
        port,
        user: "tester".to_owned(),
    }
}

fn password() -> MemoryCredentials {
    MemoryCredentials {
        password: Some(PASSWORD.to_owned()),
        ..MemoryCredentials::default()
    }
}

/// A server that carries `direct-tcpip` channels: a jump host.
async fn jump_server() -> TestServer {
    TestServer::start(ServerOptions {
        direct_tcpip: true,
        ..ServerOptions::new(host_key_a())
    })
    .await
}

fn trusting() -> Arc<MemoryGate> {
    Arc::new(MemoryGate::trusting(&host_key_a()))
}

async fn direct(server: &TestServer, options: &SessionOptions) -> Arc<SshSession> {
    let session = SshSession::connect(
        &target(server.port),
        options,
        trusting(),
        &password(),
        &CancellationToken::new(),
    )
    .await
    .expect("the first hop");
    Arc::new(session)
}

async fn via(
    parent: &Arc<SshSession>,
    server_port: u16,
    gate: Arc<MemoryGate>,
    credentials: &MemoryCredentials,
) -> Result<SshSession, SshError> {
    SshSession::connect_via(
        Arc::clone(parent),
        &target(server_port),
        &SessionOptions::default(),
        gate,
        credentials,
        &CancellationToken::new(),
    )
    .await
}

/// Runs `command` on a new channel of `session` and collects what it prints.
async fn run(session: &SshSession, command: &str) -> String {
    let mut channel = session.open_channel().await.expect("channel");
    channel.exec(true, command).await.expect("exec");
    let mut out = Vec::new();
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => out.extend_from_slice(&data),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    String::from_utf8(out).expect("utf-8")
}

#[tokio::test]
async fn a_session_runs_through_two_jumps() {
    let bastion = jump_server().await;
    let inner = jump_server().await;
    let goal = TestServer::start(ServerOptions::new(host_key_a())).await;
    let started = Instant::now();
    let first = direct(&bastion, &SessionOptions::default()).await;
    let second = Arc::new(
        via(&first, inner.port, trusting(), &password())
            .await
            .expect("the second hop"),
    );
    let session = via(&second, goal.port, trusting(), &password())
        .await
        .expect("the target");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(run(&session, "whoami").await, "ran: whoami");
    for server in [&bastion, &inner, &goal] {
        assert_eq!(server.accepts(), 1, "one TCP connection each");
    }
    assert_eq!(bastion.direct_opens(), 1);
    assert_eq!(inner.direct_opens(), 1);
    assert!(session.via().is_some_and(|hop| Arc::ptr_eq(hop, &second)));
}

#[tokio::test]
async fn a_changed_hop_key_stops_the_chain_before_any_secret() {
    let bastion = jump_server().await;
    let impostor = TestServer::start(ServerOptions {
        direct_tcpip: true,
        ..ServerOptions::new(host_key_b())
    })
    .await;
    let first = direct(&bastion, &SessionOptions::default()).await;
    let credentials = password();
    let error = via(&first, impostor.port, trusting(), &credentials)
        .await
        .err()
        .expect("refused");
    assert_eq!(error.code(), HOST_KEY_CHANGED);
    assert_eq!(impostor.auth_attempts(), 0);
    assert_eq!(credentials.asked(), 0);
}

#[tokio::test]
async fn a_jump_host_that_cannot_reach_the_next_hop_says_unreachable() {
    let bastion = jump_server().await;
    let first = direct(&bastion, &SessionOptions::default()).await;
    let closed = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        listener.local_addr().expect("address").port()
    };
    let error = via(&first, closed, trusting(), &password())
        .await
        .err()
        .expect("unreachable");
    assert_eq!(error.code(), "ssh_unreachable");
    assert!(!first.is_closed(), "the jump host stays");
}

#[tokio::test]
async fn a_frozen_jump_host_ends_the_session_behind_it() {
    let bastion = jump_server().await;
    let goal = TestServer::start(ServerOptions::new(host_key_a())).await;
    let options = SessionOptions {
        keepalive: Duration::from_millis(100),
        ..SessionOptions::default()
    };
    let first = direct(&bastion, &options).await;
    let session = via(&first, goal.port, trusting(), &password())
        .await
        .expect("the target");
    bastion.freeze();
    let rounds = u32::try_from(KEEPALIVE_MAX).expect("small");
    let limit = Duration::from_millis(100) * rounds + Duration::from_secs(5);
    tokio::time::timeout(limit, async {
        while !session.is_closed() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the target noticed");
    let error = session.open_channel().await.expect_err("lost");
    assert_eq!(error.code(), "connection_lost");
}

#[tokio::test]
async fn a_cancel_during_the_second_handshake_ends_within_a_second() {
    let bastion = jump_server().await;
    let (hanging, _held) = silent_server().await;
    let first = direct(&bastion, &SessionOptions::default()).await;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let error = SshSession::connect_via(
        Arc::clone(&first),
        &target(hanging),
        &SessionOptions::default(),
        trusting(),
        &password(),
        &cancel,
    )
    .await
    .err()
    .expect("cancelled");
    assert!(matches!(error, SshError::Cancelled), "{error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_disconnect_closes_the_whole_chain() {
    let bastion = jump_server().await;
    let goal = TestServer::start(ServerOptions::new(host_key_a())).await;
    let first = direct(&bastion, &SessionOptions::default()).await;
    let session = via(&first, goal.port, trusting(), &password())
        .await
        .expect("the target");
    session.disconnect().await;
    tokio::time::timeout(Duration::from_secs(5), first.closed())
        .await
        .expect("the jump host closed too");
}

#[tokio::test]
async fn a_direct_tcpip_channel_carries_bytes_both_ways() {
    let server = jump_server().await;
    let (echo, _echo) = echo_server().await;
    let session = direct(&server, &SessionOptions::default()).await;
    let channel = session
        .open_direct_tcpip("127.0.0.1", echo, "127.0.0.1", 40_000)
        .await
        .expect("channel");
    let mut stream = channel.into_stream();
    stream.write_all(b"ping").await.expect("write");
    let mut back = [0_u8; 4];
    stream.read_exact(&mut back).await.expect("read");
    assert_eq!(&back, b"ping");
    let refused = session
        .open_direct_tcpip("example.org", 80, "127.0.0.1", 40_001)
        .await
        .expect_err("not loopback");
    assert_eq!(refused.code(), "ssh_unreachable");
}
