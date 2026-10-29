//! The session against the in-process server: host keys, timeouts, cancelling, channels and a
//! lost connection.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use russh::{ChannelMsg, MethodKind};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    HOST_KEY_CHANGED, HOST_KEY_UNKNOWN,
    ssh::{HostKeyRefusal, SshTarget},
    testkit::{
        MemoryCredentials, MemoryGate, PASSWORD, ServerOptions, TestServer, fingerprint,
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

async fn connect_with(
    port: u16,
    options: &SessionOptions,
    gate: MemoryGate,
    credentials: &MemoryCredentials,
    cancel: &CancellationToken,
) -> Result<SshSession, SshError> {
    SshSession::connect(&target(port), options, Arc::new(gate), credentials, cancel).await
}

async fn connect(server: &TestServer, options: &SessionOptions) -> SshSession {
    connect_with(
        server.port,
        options,
        MemoryGate::trusting(&host_key_a()),
        &password(),
        &CancellationToken::new(),
    )
    .await
    .expect("connected")
}

/// Runs `command` on `channel` and collects what it prints until the channel closes.
async fn run(mut channel: SshChannel, command: &str) -> String {
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
async fn an_unknown_host_key_is_refused_before_any_secret_is_asked_for() {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let credentials = password();
    let error = connect_with(
        server.port,
        &SessionOptions::default(),
        MemoryGate::default(),
        &credentials,
        &CancellationToken::new(),
    )
    .await
    .err()
    .expect("refused");
    let SshError::HostKey {
        offered, refusal, ..
    } = &error
    else {
        panic!("a host-key refusal: {error:?}");
    };
    assert_eq!(*refusal, HostKeyRefusal::Unknown);
    assert_eq!(offered.fingerprint, fingerprint(&host_key_a()));
    assert_eq!(error.code(), HOST_KEY_UNKNOWN);
    assert_eq!(credentials.asked(), 0);
}

#[tokio::test]
async fn a_changed_host_key_stops_the_connection() {
    let server = TestServer::start(ServerOptions::new(host_key_b())).await;
    let credentials = password();
    let error = connect_with(
        server.port,
        &SessionOptions::default(),
        MemoryGate::trusting(&host_key_a()),
        &credentials,
        &CancellationToken::new(),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(error.code(), HOST_KEY_CHANGED);
    assert!(
        error.to_string().contains(&fingerprint(&host_key_b())),
        "{error}"
    );
    assert_eq!(credentials.asked(), 0);
}

/// Security rule of the wave (H3): neither a password nor a prompt answer, nor any other login
/// request, reaches a server whose key is not trusted - unknown or changed.
#[tokio::test]
async fn no_login_request_reaches_an_untrusted_server() {
    for methods in [
        vec![MethodKind::Password, MethodKind::PublicKey],
        vec![MethodKind::KeyboardInteractive],
    ] {
        for (gate, code) in [
            (MemoryGate::default(), HOST_KEY_UNKNOWN),
            (MemoryGate::trusting(&host_key_b()), HOST_KEY_CHANGED),
        ] {
            let server = TestServer::start(ServerOptions {
                methods: methods.clone(),
                ..ServerOptions::new(host_key_a())
            })
            .await;
            let credentials = password();
            let error = connect_with(
                server.port,
                &SessionOptions::default(),
                gate,
                &credentials,
                &CancellationToken::new(),
            )
            .await
            .err()
            .expect("refused");
            assert_eq!(error.code(), code);
            assert_eq!(
                server.auth_attempts(),
                0,
                "{methods:?}: a login request was sent"
            );
            assert_eq!(credentials.asked(), 0, "{methods:?}: a secret was read");
        }
    }
}

#[tokio::test]
async fn a_hanging_handshake_times_out() {
    let (port, _server) = silent_server().await;
    let options = SessionOptions {
        handshake_timeout: Duration::from_millis(200),
        ..SessionOptions::default()
    };
    let error = connect_with(
        port,
        &options,
        MemoryGate::default(),
        &password(),
        &CancellationToken::new(),
    )
    .await
    .err()
    .expect("timed out");
    assert!(
        matches!(
            error,
            SshError::Timeout {
                stage: Stage::Handshake,
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_cancelled_setup_ends_within_a_second() {
    let (port, _server) = silent_server().await;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let error = connect_with(
        port,
        &SessionOptions::default(),
        MemoryGate::default(),
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
async fn a_closed_port_is_unreachable() {
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        listener.local_addr().expect("address").port()
    };
    let error = connect_with(
        port,
        &SessionOptions::default(),
        MemoryGate::default(),
        &password(),
        &CancellationToken::new(),
    )
    .await
    .err()
    .expect("unreachable");
    assert_eq!(error.code(), "ssh_unreachable");
}

#[tokio::test]
async fn two_channels_run_at_once_over_one_tcp_connection() {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let session = connect(&server, &SessionOptions::default()).await;
    let first = session.open_channel().await.expect("first channel");
    let second = session.open_channel().await.expect("second channel");
    let (one, two) = tokio::join!(run(first, "sftp-ish"), run(second, "exec-ish"));
    assert_eq!(one, "ran: sftp-ish");
    assert_eq!(two, "ran: exec-ish");
    assert_eq!(server.accepts(), 1);
}

#[tokio::test]
async fn a_pty_shell_is_a_byte_stream() {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let session = connect(&server, &SessionOptions::default()).await;
    let channel = session.open_channel().await.expect("channel");
    let mut stream = open_pty_shell(channel, "xterm-256color", 80, 24)
        .await
        .expect("a shell");
    stream.write_all(b"login: ").await.expect("write");
    let mut echoed = [0_u8; 7];
    stream.read_exact(&mut echoed).await.expect("read");
    assert_eq!(&echoed, b"login: ");
}

#[tokio::test]
async fn the_client_neither_asks_for_nor_takes_any_forwarding() {
    let server = TestServer::start(ServerOptions {
        probe_forwarding: true,
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let session = connect(&server, &SessionOptions::default()).await;
    let channel = session.open_channel().await.expect("channel");
    let _shell = open_pty_shell(channel, "xterm-256color", 80, 24)
        .await
        .expect("a shell");
    let probes = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let probes = server.probes();
            if probes.len() == 4 {
                return probes;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the server tried every channel");
    assert!(
        probes.iter().all(|(_, accepted)| !accepted),
        "the client took one: {probes:?}"
    );
    assert!(
        server.forward_requests().is_empty(),
        "{:?}",
        server.forward_requests()
    );
    assert!(!session.is_closed(), "refusing does not end the session");
}

#[tokio::test]
async fn a_silent_server_is_noticed_by_the_keepalive() {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let options = SessionOptions {
        keepalive: Duration::from_millis(100),
        ..SessionOptions::default()
    };
    let session = connect(&server, &options).await;
    assert!(!session.is_closed());
    server.freeze();
    tokio::time::timeout(Duration::from_secs(5), session.closed())
        .await
        .expect("the keepalive gave up");
    assert!(session.is_closed());
    let error = session.open_channel().await.expect_err("lost");
    assert_eq!(error.code(), "connection_lost");
}

#[tokio::test]
async fn a_disconnect_closes_the_session() {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let session = connect(&server, &SessionOptions::default()).await;
    session.disconnect().await;
    tokio::time::timeout(Duration::from_secs(5), session.closed())
        .await
        .expect("closed");
}
