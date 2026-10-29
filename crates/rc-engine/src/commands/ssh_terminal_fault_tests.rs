//! A terminal whose SSH connection breaks (RC-080-04): the server ends the connection, or goes
//! silent as a paused or unplugged server does. The terminal ends without an exit code, the
//! session is announced as lost - what the window shows -, nothing panics, and the engine goes on
//! answering and opens the terminal again on a new connection.

use std::time::Duration;

use tokio::sync::broadcast;

use super::tests::{harness, open_ssh, opened, shows};
use crate::{
    Command, EngineEvent, Outcome, SessionChange, SessionPrefs, SessionState, TerminalInfo,
};

/// Waits until `connection`'s session is announced as lost, at most `limit`.
async fn lost(events: &mut broadcast::Receiver<EngineEvent>, limit: Duration) {
    tokio::time::timeout(limit, async {
        loop {
            if let Ok(EngineEvent::Session {
                change: SessionChange::Lost,
                ..
            }) = events.recv().await
            {
                return;
            }
        }
    })
    .await
    .expect("the session is announced as lost");
}

/// The terminal `info` ends within `limit`, without an exit code.
async fn ends_without_a_code(engine: &crate::Engine, info: &TerminalInfo, limit: Duration) {
    let session = engine.terminal_session(info.terminal).expect("the session");
    let code = tokio::time::timeout(limit, session.wait_exit())
        .await
        .expect("the terminal ends");
    assert_eq!(code, None);
    assert_eq!(session.state(), SessionState::Exited { code: None });
}

/// After the loss the engine still answers, and a new terminal connects anew.
async fn opens_again(harness: &super::tests::Harness) {
    let listed = harness
        .engine
        .dispatch(Command::ListTerminals {})
        .await
        .expect("the engine answers");
    assert!(matches!(listed, Outcome::Terminals { .. }), "{listed:?}");
    let again = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    shows(&harness.engine, again.terminal, "tester@test:~$").await;
    assert_eq!(harness.server.accepts(), 2, "a new connection");
}

#[tokio::test]
async fn a_connection_the_server_ends_ends_the_terminal_and_is_announced_lost() {
    let harness = harness().await;
    let mut events = harness.engine.subscribe();
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    shows(&harness.engine, info.terminal, "tester@test:~$").await;
    harness
        .engine
        .dispatch(Command::SendInput {
            terminal: info.terminal,
            text: "drop\r".into(),
        })
        .await
        .expect("typed");
    ends_without_a_code(&harness.engine, &info, Duration::from_secs(10)).await;
    lost(&mut events, Duration::from_secs(10)).await;
    opens_again(&harness).await;
}

#[tokio::test]
async fn a_server_that_goes_silent_ends_the_terminal_once_the_keepalive_gives_up() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::SetSessionPrefs {
            id: harness.id,
            prefs: SessionPrefs {
                keepalive_secs: 1,
                ..SessionPrefs::default()
            },
        })
        .await
        .expect("settings stored");
    let mut events = harness.engine.subscribe();
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    shows(&harness.engine, info.terminal, "tester@test:~$").await;
    harness.server.freeze();
    // Typing into a silent connection is no error of its own: the bytes are swallowed.
    harness
        .engine
        .dispatch(Command::SendInput {
            terminal: info.terminal,
            text: "pwd\r".into(),
        })
        .await
        .expect("typed");
    lost(&mut events, Duration::from_secs(20)).await;
    ends_without_a_code(&harness.engine, &info, Duration::from_secs(10)).await;
    opens_again(&harness).await;
}
