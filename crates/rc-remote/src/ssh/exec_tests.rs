//! Commands against the in-process server: the streams, the exit code and signal, the time and
//! size limits, standard input, and a run dropped half-way.

use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    ssh::{SessionOptions, SshSession, SshTarget},
    testkit::{MemoryCredentials, MemoryGate, PASSWORD, ServerOptions, TestServer, host_key_a},
};

async fn server_and_session() -> (TestServer, SshSession) {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let target = SshTarget {
        host: "127.0.0.1".to_owned(),
        port: server.port,
        user: "tester".to_owned(),
    };
    let credentials = MemoryCredentials {
        password: Some(PASSWORD.to_owned()),
        ..MemoryCredentials::default()
    };
    let session = SshSession::connect(
        &target,
        &SessionOptions::default(),
        Arc::new(MemoryGate::trusting(&host_key_a())),
        &credentials,
        &CancellationToken::new(),
    )
    .await
    .expect("connected");
    (server, session)
}

async fn run(
    session: &SshSession,
    command: &str,
    stdin: Option<&[u8]>,
    limits: ExecLimits,
) -> Result<ExecOutput, SshError> {
    let channel = session.open_channel().await.expect("a channel");
    exec_command(channel, command, stdin, limits).await
}

fn within(timeout: Duration) -> ExecLimits {
    ExecLimits {
        timeout,
        ..ExecLimits::default()
    }
}

/// Waits up to five seconds for `done`.
async fn eventually(done: impl Fn() -> bool) -> bool {
    for _ in 0..500 {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn the_streams_and_the_exit_code_come_back_apart() {
    let (_server, session) = server_and_session().await;
    let output = run(
        &session,
        "sh -c 'echo out; echo err >&2; exit 3'",
        None,
        ExecLimits::default(),
    )
    .await
    .expect("ran");
    assert_eq!(output.stdout, "out\n");
    assert_eq!(output.stderr, "err\n");
    assert_eq!(output.exit_code, Some(3));
    assert_eq!(output.exit_signal, None);
    assert!(!output.timed_out && !output.stdout_truncated && !output.stderr_truncated);
}

#[tokio::test]
async fn a_command_ended_by_a_signal_names_it() {
    let (_server, session) = server_and_session().await;
    let output = run(
        &session,
        "sh -c 'echo partial; kill -KILL $$'",
        None,
        ExecLimits::default(),
    )
    .await
    .expect("ran");
    assert_eq!(output.stdout, "partial\n");
    assert_eq!(output.exit_signal.as_deref(), Some("KILL"));
    assert_eq!(output.exit_code, None);
}

#[tokio::test]
async fn a_command_over_its_time_is_stopped_and_the_session_stays() {
    let (server, session) = server_and_session().await;
    let output = run(
        &session,
        "sh -c 'echo started; sleep 30'",
        None,
        within(Duration::from_millis(500)),
    )
    .await
    .expect("a timeout is an answer");
    assert!(output.timed_out);
    assert_eq!(output.stdout, "started\n", "what came in time is kept");
    assert_eq!(output.exit_code, None);
    assert!(
        eventually(|| server.exec_runs().stopped() == 1).await,
        "the channel was closed"
    );
    assert!(server.exec_runs().signals().contains(&"TERM".to_owned()));

    assert!(!session.is_closed());
    let again = run(&session, "sh -c 'exit 0'", None, ExecLimits::default())
        .await
        .expect("the session still runs commands");
    assert_eq!(again.exit_code, Some(0));
}

#[tokio::test]
async fn output_over_the_limit_is_cut_and_marked() {
    let (_server, session) = server_and_session().await;
    let limits = ExecLimits {
        max_output: 1000,
        ..ExecLimits::default()
    };
    let output = run(
        &session,
        "sh -c 'yes | head -c 300000; yes | head -c 10 >&2'",
        None,
        limits,
    )
    .await
    .expect("ran");
    assert_eq!(output.stdout.len(), 1000);
    assert!(output.stdout_truncated);
    assert_eq!(output.stderr, "y\n".repeat(5));
    assert!(!output.stderr_truncated);
    assert_eq!(output.exit_code, Some(0), "the rest was read to the end");
}

#[tokio::test]
async fn standard_input_is_sent_and_closed() {
    let (_server, session) = server_and_session().await;
    let echoed = run(&session, "cat", Some(b"hello\n"), ExecLimits::default())
        .await
        .expect("ran");
    assert_eq!(echoed.stdout, "hello\n");
    assert_eq!(echoed.exit_code, Some(0));
    let empty = run(&session, "cat", None, within(Duration::from_secs(5)))
        .await
        .expect("ran");
    assert!(!empty.timed_out, "without input, the input ends at once");
    assert_eq!(empty.stdout, "");
}

#[tokio::test]
async fn a_run_dropped_half_way_is_stopped_and_its_channel_closed() {
    let (server, session) = server_and_session().await;
    let channel = session.open_channel().await.expect("a channel");
    let run = exec_command(channel, "sh -c 'sleep 30'", None, ExecLimits::default());
    let dropped = tokio::time::timeout(Duration::from_millis(100), run).await;
    assert!(dropped.is_err(), "still running when dropped");
    assert!(eventually(|| server.exec_runs().stopped() == 1).await);
}

#[test]
fn keep_takes_what_fits_and_marks_the_rest() {
    let (mut buffer, mut truncated) = (Vec::new(), false);
    keep(&mut buffer, &mut truncated, b"abc", 4);
    assert_eq!((buffer.as_slice(), truncated), (&b"abc"[..], false));
    keep(&mut buffer, &mut truncated, b"de", 4);
    assert_eq!((buffer.as_slice(), truncated), (&b"abcd"[..], true));
    keep(&mut buffer, &mut truncated, b"f", 4);
    assert_eq!(buffer, b"abcd");
}
