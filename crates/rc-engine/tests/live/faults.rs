//! A real server that fails in the middle of the work (RC-080-04, stage 2): its container paused
//! (the network hangs, nothing closes), its sessions killed, the container killed or stopped.
//! A listing keeps what arrived and says why, an upload waits resumable and goes on from its part
//! file, a terminal ends and the session is announced as lost; nothing panics, the engine goes on
//! answering, and the next use connects again. Each case has a server of its own.

use std::time::{Duration, Instant};

use rc_engine::{
    Command, Engine, EngineEvent, JobOptions, JobState, Outcome, PanelId, SessionPrefs,
    SessionState, TermId,
};
use tokio::sync::broadcast::Receiver;

use crate::live::{
    completed, connect_trusting, connection, engine, finished, local_sha256, random_file, sftp,
    transfer, under_way, until, workdir,
};
use crate::live_or_skip;
use crate::reconnect::lost;
use crate::terminal::{open, screen, shell_answers, type_in, until_screen};

/// The entries of `/home/tester/many` (ctl.sh).
const MANY: usize = 100_000;

async fn keepalive(engine: &Engine, id: rc_engine::ConnectionId, secs: u32) {
    engine
        .dispatch(Command::SetSessionPrefs {
            id,
            prefs: SessionPrefs {
                keepalive_secs: secs,
                ..SessionPrefs::default()
            },
        })
        .await
        .expect("keepalive set");
}

/// Shows `path` in the left panel; the panel's snapshot as the command answers it.
async fn navigate(engine: &Engine, path: &str) -> rc_engine::PanelSnapshot {
    let shown = engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: path.to_owned(),
        })
        .await;
    let Ok(Outcome::Panel(snapshot)) = shown else {
        panic!("a panel, got {shown:?}");
    };
    snapshot
}

/// Waits for the left panel's listing to break off, `limit` at most: its code and when.
async fn listing_failed(
    engine: &Engine,
    events: &mut Receiver<EngineEvent>,
    limit: Duration,
) -> (String, Duration) {
    let started = Instant::now();
    let code = tokio::time::timeout(limit, async {
        loop {
            if let Ok(EngineEvent::ListingFailed {
                panel: PanelId::Left,
                code,
            }) = events.recv().await
            {
                return code;
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        let state = engine.panel(PanelId::Left);
        panic!(
            "the broken listing is never announced: still listing {:?}, {} entries",
            state.listing,
            state.entries.len()
        )
    });
    (code, started.elapsed())
}

/// The left panel kept part of the listing and lists no more.
fn kept_what_arrived(engine: &Engine) -> usize {
    let state = engine.panel(PanelId::Left);
    assert!(state.listing.is_none(), "the listing is over");
    let kept = state.entries.len();
    assert!(kept > 0 && kept < MANY, "part of it: {kept}");
    kept
}

/// The left panel lists the whole folder again, from its parent, 120 s at most.
async fn lists_it_whole(engine: &Engine, path: &str) {
    navigate(engine, "..").await;
    navigate(engine, path).await;
    let started = Instant::now();
    while engine.panel(PanelId::Left).listing.is_some() {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "never complete"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(engine.panel(PanelId::Left).entries.len(), MANY);
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_listing_whose_server_hangs_or_drops_keeps_what_arrived_says_why_and_lists_again() {
    let live = live_or_skip!();
    let dir = workdir();
    let home = dir.path().display().to_string();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-faults-list", "faults-list").await;
    keepalive(&engine, id, 2).await;
    connect_trusting(&engine, id).await;
    let path = sftp("live-faults-list", "/home/tester/many");
    let mut events = engine.subscribe();

    let shown = navigate(&engine, &path).await;
    assert!(shown.loading, "still arriving when it is shown: {shown:?}");
    live.ctl(&["pause", "faults-list"]);
    let (code, after) = listing_failed(&engine, &mut events, Duration::from_secs(90)).await;
    let kept = kept_what_arrived(&engine);
    eprintln!("hanging server: {code} after {after:?}, {kept} entries kept");
    // The engine answers while the server hangs: the other panel goes elsewhere at once.
    let started = Instant::now();
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: home,
        })
        .await
        .expect("the other panel answers");
    assert!(started.elapsed() < Duration::from_secs(2));
    live.ctl(&["unpause", "faults-list"]);
    lists_it_whole(&engine, &path).await;

    navigate(&engine, "..").await;
    let shown = navigate(&engine, &path).await;
    assert!(shown.loading, "still arriving when it is shown: {shown:?}");
    // The server ends the session: the connection closes at once (the client sees it).
    let killed = live.ctl(&["kill-sessions", "faults-list"]);
    assert!(killed.contains("killed"), "a session to kill");
    let (code, after) = listing_failed(&engine, &mut events, Duration::from_secs(90)).await;
    let kept = kept_what_arrived(&engine);
    eprintln!("dropped session: {code} after {after:?}, {kept} entries kept");
    lists_it_whole(&engine, &path).await;
    // The READDIR in flight fails with the session, not after the 30 s request timeout.
    assert_eq!(
        code, "connection_lost",
        "a closed connection is named as such"
    );
    assert!(
        after < Duration::from_secs(10),
        "and said at once, not after {after:?}"
    );
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn an_upload_whose_server_is_killed_waits_resumable_and_goes_on_from_its_part() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-faults-copy", "faults-copy").await;
    connect_trusting(&engine, id).await;
    let mine = live.remote_dir("faults-copy", "t-up");
    let source = dir.path().join("big.bin");
    random_file(&source, live.big, 23);
    let digest = local_sha256(&source);

    let job = transfer(
        &engine,
        vec![source.display().to_string()],
        sftp("live-faults-copy", &mine),
        JobOptions::default(),
        false,
    )
    .await;
    let before = under_way(&engine, job, live.big).await;
    live.ctl(&["kill", "faults-copy"]);
    let paused = until(
        &engine,
        job,
        Duration::from_secs(180),
        "paused as resumable",
        |snapshot| snapshot.state == JobState::Paused && snapshot.resumable,
    )
    .await;
    eprintln!(
        "killed at {} bytes; retries spent: {:?} {:?} {:?}",
        before.progress.bytes_done, paused.progress, paused.failures, paused.question
    );
    assert!(
        !paused.failures.is_empty(),
        "a paused job says why it waits"
    );
    live.ctl(&["start", "faults-copy"]);
    assert_eq!(
        live.remote_names("faults-copy", &mine),
        ["big.bin.rcpart"],
        "only the part file"
    );
    let part = live.ctl(&[
        "exec",
        "faults-copy",
        &format!("stat -c %s '{mine}/big.bin.rcpart'"),
    ]);
    eprintln!("part file after the kill: {} bytes", part.trim());
    engine
        .dispatch(Command::OpsResume { job })
        .await
        .expect("resumed");
    completed(&finished(&engine, job, Duration::from_secs(240)).await);
    assert_eq!(live.remote_names("faults-copy", &mine), ["big.bin"]);
    assert_eq!(
        live.remote_sha256("faults-copy", &format!("{mine}/big.bin")),
        digest,
        "byte for byte"
    );
}

/// The terminal `terminal` ends without an exit code, `limit` at most.
async fn ends_without_a_code(engine: &Engine, terminal: TermId, limit: Duration) {
    let session = engine.terminal_session(terminal).expect("the session");
    let code = tokio::time::timeout(limit, session.wait_exit())
        .await
        .expect("the terminal ends");
    assert_eq!(code, None);
    assert_eq!(session.state(), SessionState::Exited { code: None });
}

/// A new terminal on the connection, with `top` running in it.
async fn top_running(engine: &Engine, id: rc_engine::ConnectionId, word: &str) -> TermId {
    let terminal = open(engine, id, 30, 100).await;
    shell_answers(engine, terminal, word).await;
    type_in(engine, terminal, "top\r").await;
    until_screen(engine, terminal, "top", |lines| {
        lines.iter().any(|line| line.starts_with("Mem:"))
    })
    .await;
    terminal
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_terminal_whose_server_hangs_or_stops_ends_is_announced_lost_and_opens_again() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-faults-term", "faults-term").await;
    keepalive(&engine, id, 1).await;
    connect_trusting(&engine, id).await;
    let mut events = engine.subscribe();

    let terminal = top_running(&engine, id, "READY").await;
    live.ctl(&["pause", "faults-term"]);
    let noticed = lost(&mut events, id, Duration::from_secs(30)).await;
    ends_without_a_code(&engine, terminal, Duration::from_secs(10)).await;
    eprintln!(
        "hanging server: lost after {noticed:?}; the screen:\n{}",
        screen(&engine, terminal).await.join("\n")
    );
    let listed = engine
        .dispatch(Command::ListTerminals {})
        .await
        .expect("the engine answers");
    assert!(matches!(listed, Outcome::Terminals { .. }), "{listed:?}");
    live.ctl(&["unpause", "faults-term"]);
    let terminal = top_running(&engine, id, "AGAIN").await;
    assert_eq!(live.logins("faults-term"), 2, "a new connection");

    live.ctl(&["stop", "faults-term"]);
    let noticed = lost(&mut events, id, Duration::from_secs(30)).await;
    ends_without_a_code(&engine, terminal, Duration::from_secs(10)).await;
    eprintln!("stopped server: lost after {noticed:?}");
    live.ctl(&["start", "faults-term"]);
    top_running(&engine, id, "BACK").await;
    assert_eq!(
        live.logins("faults-term"),
        3,
        "a new connection after the restart"
    );
}
