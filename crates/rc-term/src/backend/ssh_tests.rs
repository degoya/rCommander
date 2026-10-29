//! The SSH back end against the in-process shell server of `rc-remote`'s testkit: the PTY
//! request, `window-change`, the exit status, a channel or connection that ends without one, and
//! the end the terminal asks for.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use rc_remote::{
    SshSession,
    ssh::{SessionOptions, SshTarget, start_pty_shell},
    testkit::{MemoryCredentials, MemoryGate, PASSWORD, PtyRequest, ShellServer, host_key_a},
};
use tokio_util::sync::CancellationToken;

use super::spawn_ssh;
use crate::{SessionOptions as TermOptions, TermError, TermId, TermSession, TermSize};

/// A logged-in session on `server`.
async fn connect(server: &ShellServer) -> SshSession {
    let target = SshTarget {
        host: "127.0.0.1".to_owned(),
        port: server.port,
        user: "tester".to_owned(),
    };
    let credentials = MemoryCredentials {
        password: Some(PASSWORD.to_owned()),
        ..MemoryCredentials::default()
    };
    SshSession::connect(
        &target,
        &SessionOptions::default(),
        Arc::new(MemoryGate::trusting(&host_key_a())),
        &credentials,
        &CancellationToken::new(),
    )
    .await
    .expect("connected")
}

/// A terminal session of `size` on a new shell channel of `ssh`; `keep` lives with the channel.
async fn open(ssh: &SshSession, size: TermSize, keep: Arc<()>) -> TermSession {
    let mut channel = ssh.open_channel().await.expect("a channel");
    start_pty_shell(
        &mut channel,
        "xterm-256color",
        u32::from(size.cols),
        u32::from(size.rows),
    )
    .await
    .expect("a PTY and a shell");
    TermSession::start(
        TermId::next(),
        spawn_ssh(channel, keep),
        TermOptions {
            title: "box".into(),
            shell: "ssh".into(),
            cwd: PathBuf::from("~"),
            size,
            scrollback: 100,
        },
    )
}

/// Waits until the screen shows `needle`.
async fn shows(session: &TermSession, needle: &str) {
    let started = Instant::now();
    loop {
        let screen = session.screen(100);
        let text = [screen.scrollback, screen.lines].concat().join("\n");
        if text.contains(needle) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{needle:?} never showed:\n{text}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Waits until `check` holds.
async fn eventually(what: &str, check: impl Fn() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < Duration::from_secs(5), "never: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn ends(session: &TermSession) -> Option<u32> {
    tokio::time::timeout(Duration::from_secs(5), session.wait_exit())
        .await
        .expect("the shell ends")
}

#[tokio::test]
async fn the_shell_gets_its_pty_its_new_size_and_reports_its_exit_status() {
    let server = ShellServer::start(host_key_a()).await;
    let ssh = connect(&server).await;
    let keep = Arc::new(());
    let session = open(
        &ssh,
        TermSize {
            rows: 30,
            cols: 100,
        },
        Arc::clone(&keep),
    )
    .await;
    shows(&session, "tester@test:~$").await;
    assert_eq!(
        server.ptys(),
        vec![PtyRequest {
            term: "xterm-256color".to_owned(),
            cols: 100,
            rows: 30,
        }]
    );

    session
        .resize(TermSize {
            rows: 40,
            cols: 120,
        })
        .expect("resized");
    eventually("the server saw the new size", || {
        server.resizes() == vec![(120, 40)]
    })
    .await;

    session.write(b"whoami\r".to_vec()).expect("typed");
    shows(&session, "\ntester\n").await;
    session.write(b"exit 3\r".to_vec()).expect("typed");
    assert_eq!(ends(&session).await, Some(3));
    assert_eq!(
        Arc::strong_count(&keep),
        1,
        "the channel let go of its lease"
    );
    assert!(matches!(
        session.write(b"x".to_vec()),
        Err(TermError::Exited)
    ));
    assert_eq!(server.lines(), ["whoami", "exit 3"]);
}

#[tokio::test]
async fn a_channel_closed_without_a_status_ends_without_a_code() {
    let server = ShellServer::start(host_key_a()).await;
    let ssh = connect(&server).await;
    let session = open(&ssh, TermSize::default(), Arc::new(())).await;
    shows(&session, "$").await;
    session.write(b"hangup\r".to_vec()).expect("typed");
    assert_eq!(ends(&session).await, None);
}

#[tokio::test]
async fn a_connection_that_ends_ends_the_terminal() {
    let server = ShellServer::start(host_key_a()).await;
    let ssh = connect(&server).await;
    let session = open(&ssh, TermSize::default(), Arc::new(())).await;
    shows(&session, "$").await;
    session.write(b"drop\r".to_vec()).expect("typed");
    assert_eq!(ends(&session).await, None);
    eventually("the connection is gone", || ssh.is_closed()).await;
}

#[tokio::test]
async fn shutdown_closes_the_channel_and_keeps_the_connection() {
    let server = ShellServer::start(host_key_a()).await;
    let ssh = connect(&server).await;
    let keep = Arc::new(());
    let session = open(&ssh, TermSize::default(), Arc::clone(&keep)).await;
    shows(&session, "$").await;
    session.shutdown();
    assert_eq!(ends(&session).await, None);
    assert_eq!(Arc::strong_count(&keep), 1);
    assert!(!ssh.is_closed(), "only the channel ended");
    let again = open(&ssh, TermSize::default(), Arc::new(())).await;
    shows(&again, "$").await;
    assert_eq!(server.accepts(), 1, "both terminals on one TCP connection");
}
