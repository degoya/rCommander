//! The pool: channels sharing one connection, a lost session and its reconnect, idle close.

use std::time::Duration;

use tokio::{io::AsyncReadExt as _, sync::broadcast};

use super::tests::{drain, harness};
use crate::{EngineEvent, SessionChange, SessionChannel, SessionPrefs, SshChannel, SshState};

/// Runs `command` and collects what it prints.
async fn run(channel: SshChannel, command: &str) -> String {
    channel.exec(true, command).await.expect("exec");
    let mut out = Vec::new();
    channel
        .into_stream()
        .read_to_end(&mut out)
        .await
        .expect("read");
    String::from_utf8(out).expect("utf-8")
}

/// The next session change announced, within `limit`.
async fn next_change(
    events: &mut broadcast::Receiver<EngineEvent>,
    limit: Duration,
) -> SessionChange {
    tokio::time::timeout(limit, async {
        loop {
            if let Ok(EngineEvent::Session { change, .. }) = events.recv().await {
                return change;
            }
        }
    })
    .await
    .expect("a session change")
}

#[tokio::test]
async fn two_channels_share_one_tcp_connection() {
    let harness = harness(None, true).await;
    let SessionChannel {
        channel: first,
        lease: first_lease,
    } = harness.engine.ssh_channel(harness.id).await.expect("first");
    let SessionChannel {
        channel: second,
        lease: second_lease,
    } = harness
        .engine
        .ssh_channel(harness.id)
        .await
        .expect("second");
    assert_eq!(
        harness
            .engine
            .ssh_session(harness.id)
            .map(|status| status.channels),
        Some(2)
    );
    let (one, two) = tokio::join!(run(first, "sftp"), run(second, "exec"));
    assert_eq!(one, "ran: sftp");
    assert_eq!(two, "ran: exec");
    assert_eq!(harness.server.accepts(), 1, "one TCP connection");
    drop((first_lease, second_lease));
    assert_eq!(
        harness
            .engine
            .ssh_session(harness.id)
            .map(|status| status.channels),
        Some(0)
    );
}

#[tokio::test]
async fn a_lost_session_is_announced_and_the_next_use_connects_again() {
    let prefs = SessionPrefs {
        keepalive_secs: 1,
        ..SessionPrefs::default()
    };
    let harness = harness(Some(prefs), true).await;
    let mut events = harness.engine.subscribe();
    let opened = harness
        .engine
        .ssh_channel(harness.id)
        .await
        .expect("channel");
    assert_eq!(
        next_change(&mut events, Duration::from_secs(1)).await,
        SessionChange::Connected
    );
    harness.server.freeze();
    assert_eq!(
        next_change(&mut events, Duration::from_secs(15)).await,
        SessionChange::Lost
    );
    assert_eq!(harness.engine.ssh_session(harness.id), None);
    drop(opened);
    let again = harness
        .engine
        .ssh_channel(harness.id)
        .await
        .expect("connected again");
    assert_eq!(run(again.channel, "after").await, "ran: after");
    assert_eq!(harness.server.accepts(), 2);
    assert!(
        !drain(&mut events)
            .iter()
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "no question while the vault is unlocked"
    );
}

#[tokio::test]
async fn an_unused_session_closes_after_its_idle_time() {
    let prefs = SessionPrefs {
        idle_close_secs: 1,
        ..SessionPrefs::default()
    };
    let harness = harness(Some(prefs), true).await;
    let mut events = harness.engine.subscribe();
    let opened = harness
        .engine
        .ssh_channel(harness.id)
        .await
        .expect("channel");
    assert_eq!(
        next_change(&mut events, Duration::from_secs(1)).await,
        SessionChange::Connected
    );
    // A channel keeps it open past its idle time.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        harness
            .engine
            .ssh_session(harness.id)
            .map(|status| status.state),
        Some(SshState::Connected)
    );
    drop(opened);
    assert_eq!(
        next_change(&mut events, Duration::from_secs(5)).await,
        SessionChange::Closed
    );
    assert_eq!(harness.engine.ssh_session(harness.id), None);
}
