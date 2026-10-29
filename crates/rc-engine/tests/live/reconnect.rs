//! A kept session on a real server (RC-060-01): the keepalive holds an idle connection open,
//! notices a server that stops answering (a paused container), and a restarted server is noticed
//! too; either way the next use connects again, without a question about the known host key.
//! The `alive` server and the `lost` server each belong to one test, which disturbs it.

use std::time::{Duration, Instant};

use rc_engine::{
    Command, ConnectionId, Engine, EngineEvent, Outcome, SessionChange, SessionPrefs, SshState,
};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

use crate::live::{connect_trusting, connection, engine, sftp, workdir};
use crate::live_or_skip;

/// Waits until the session of `connection` is announced as lost, `limit` at most; how long it
/// took.
pub async fn lost(
    events: &mut Receiver<EngineEvent>,
    connection: ConnectionId,
    limit: Duration,
) -> Duration {
    let started = Instant::now();
    tokio::time::timeout(limit, async {
        loop {
            // A lagging receiver missed some events; the loss is still to come or to be seen.
            if let Ok(EngineEvent::Session {
                connection: which,
                change: SessionChange::Lost,
            }) = events.recv().await
                && which == connection
            {
                return;
            }
        }
    })
    .await
    .expect("the session is announced as lost");
    started.elapsed()
}

/// Every `Session` change of `connection` announced so far.
fn changes(events: &mut Receiver<EngineEvent>, connection: ConnectionId) -> Vec<SessionChange> {
    let mut seen = Vec::new();
    loop {
        match events.try_recv() {
            Ok(EngineEvent::Session {
                connection: which,
                change,
            }) if which == connection => seen.push(change),
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(_) => return seen,
        }
    }
}

/// `docs` on the server, read by `panel.list_dir` the way the MCP tool `list_dir` does.
async fn lists(engine: &Engine, name: &str) {
    let listing = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some(sftp(name, "/home/tester/docs")),
        })
        .await;
    assert!(
        matches!(listing, Ok(Outcome::Listing { .. })),
        "{listing:?}"
    );
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn the_keepalive_holds_an_idle_session_notices_a_hang_and_a_restart_and_the_next_use_connects()
 {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-alive", "alive").await;
    engine
        .dispatch(Command::SetSessionPrefs {
            id,
            prefs: SessionPrefs {
                keepalive_secs: 1,
                ..SessionPrefs::default()
            },
        })
        .await
        .expect("keepalive every second");
    connect_trusting(&engine, id).await;
    let mut events = engine.subscribe();
    lists(&engine, "live-alive").await;

    // Idle well past several keepalives: a server that answers them keeps the session.
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(changes(&mut events, id), [], "nothing happened to it");
    let status = engine.ssh_session(id).expect("a session");
    assert_eq!(status.state, SshState::Connected, "{status:?}");
    lists(&engine, "live-alive").await;
    assert_eq!(live.logins("alive"), 1, "the one session served all of it");

    live.ctl(&["pause", "alive"]);
    let noticed = lost(&mut events, id, Duration::from_secs(30)).await;
    eprintln!("a server that stopped answering was noticed after {noticed:?}");
    live.ctl(&["unpause", "alive"]);
    lists(&engine, "live-alive").await;
    assert_eq!(live.logins("alive"), 2, "connected again");

    live.ctl(&["stop", "alive"]);
    let noticed = lost(&mut events, id, Duration::from_secs(30)).await;
    eprintln!("a stopped server was noticed after {noticed:?}");
    live.ctl(&["start", "alive"]);
    lists(&engine, "live-alive").await;
    assert_eq!(live.logins("alive"), 3, "connected again after the restart");
    assert!(
        engine.host_key_questions().is_empty(),
        "the same host key is no question"
    );
    let status = engine.ssh_session(id).expect("a session");
    assert_eq!(status.state, SshState::Connected, "{status:?}");
}

/// RC-060-01: the next use after a noticed loss connects again - the kept SFTP backend closes
/// with the pool's session, so the first listing after it does not answer `connection_lost`
/// once more.
#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn the_first_listing_after_an_announced_loss_connects_again() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-lost", "lost").await;
    connect_trusting(&engine, id).await;
    lists(&engine, "live-lost").await;
    let mut events = engine.subscribe();
    let killed = live.ctl(&["kill-sessions", "lost"]);
    assert!(killed.contains("killed"), "a session to kill");
    lost(&mut events, id, Duration::from_secs(10)).await;
    lists(&engine, "live-lost").await;
}
