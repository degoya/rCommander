//! A download that loses its connection: sessions killed on the server, a network that hangs, a
//! server that stays away until the retries are spent, an app that ends in the middle. Each runs
//! on a server of its own, whose SFTP log names the offset of every read: a transfer that goes
//! on from its part file reads offset 0 of the file once, one that starts over reads it again.

use std::{path::Path, time::Duration};

use rc_engine::{Command, JobOptions, JobState, SessionPrefs};

use crate::live::{
    Live, completed, connect_trusting, connection, engine, finished, local_sha256, sftp, transfer,
    under_way, until, workdir,
};
use crate::live_or_skip;

const LONG: Duration = Duration::from_secs(240);

/// How often the server was asked for the start of `big.bin`.
fn reads_from_the_start(live: &Live, role: &str) -> usize {
    live.ctl(&["sftp-log", role])
        .lines()
        .filter(|line| line.contains("big.bin") && line.contains(" off 0 len "))
        .count()
}

/// The file came down whole, without the part file, and was read from the start only once.
fn came_down_whole(live: &Live, role: &str, down: &Path) {
    assert_eq!(
        local_sha256(&down.join("big.bin")),
        live.remote_sha256(role, "/home/tester/big.bin"),
        "byte for byte"
    );
    assert!(!down.join("big.bin.rcpart").exists());
    assert_eq!(
        reads_from_the_start(live, role),
        1,
        "went on from the part file instead of starting over"
    );
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_dropped_connection_is_retried_and_the_download_goes_on_from_its_part() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-drop", "drop").await;
    connect_trusting(&engine, id).await;
    let down = dir.path().join("down");
    std::fs::create_dir(&down).expect("mkdir");

    let job = transfer(
        &engine,
        vec![sftp("live-drop", "/home/tester/big.bin")],
        down.display().to_string(),
        JobOptions::default(),
        false,
    )
    .await;
    let before = under_way(&engine, job, live.big).await;
    let killed = live.ctl(&["kill-sessions", "drop"]);
    eprintln!("at {} bytes:\n{killed}", before.progress.bytes_done);
    assert!(killed.contains("killed"), "a session to kill");
    let done = finished(&engine, job, LONG).await;
    completed(&done);
    assert!(live.logins("drop") >= 2, "it logged in again");
    came_down_whole(&live, "drop", &down);
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_hanging_network_is_noticed_and_the_download_goes_on_once_it_is_back() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-hang", "hang").await;
    engine
        .dispatch(Command::SetSessionPrefs {
            id,
            prefs: SessionPrefs {
                keepalive_secs: 2,
                ..SessionPrefs::default()
            },
        })
        .await
        .expect("keepalive every 2 s");
    connect_trusting(&engine, id).await;
    let down = dir.path().join("down");
    std::fs::create_dir(&down).expect("mkdir");

    let job = transfer(
        &engine,
        vec![sftp("live-hang", "/home/tester/big.bin")],
        down.display().to_string(),
        JobOptions::default(),
        false,
    )
    .await;
    under_way(&engine, job, live.big).await;
    live.ctl(&["pause", "hang"]);
    tokio::time::sleep(Duration::from_secs(15)).await;
    let during = engine.ops().job(job).expect("known");
    eprintln!(
        "after 15 s without network: {:?} {:?}",
        during.state, during.progress
    );
    live.ctl(&["unpause", "hang"]);
    let done = finished(&engine, job, LONG).await;
    completed(&done);
    eprintln!("logins on the server: {}", live.logins("hang"));
    came_down_whole(&live, "hang", &down);
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn spent_retries_leave_a_resumable_job_that_resume_finishes() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-spent", "spent").await;
    connect_trusting(&engine, id).await;
    let down = dir.path().join("down");
    std::fs::create_dir(&down).expect("mkdir");

    let job = transfer(
        &engine,
        vec![sftp("live-spent", "/home/tester/big.bin")],
        down.display().to_string(),
        JobOptions::default(),
        false,
    )
    .await;
    under_way(&engine, job, live.big).await;
    live.ctl(&["stop", "spent"]);
    let paused = until(
        &engine,
        job,
        Duration::from_secs(180),
        "paused as resumable",
        |snapshot| snapshot.state == JobState::Paused && snapshot.resumable,
    )
    .await;
    eprintln!("retries spent: {:?}", paused.progress);
    assert!(down.join("big.bin.rcpart").exists(), "the part file stays");
    live.ctl(&["start", "spent"]);
    engine
        .dispatch(Command::OpsResume { job })
        .await
        .expect("resumed");
    completed(&finished(&engine, job, LONG).await);
    came_down_whole(&live, "spent", &down);
}

/// The first run is a runtime of its own that is shut down mid-download, its engine never
/// dropped: as close to a quit (or a crash) as one process gets.
#[test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
fn a_kept_job_comes_back_paused_after_a_restart_and_resumes() {
    let live = live_or_skip!();
    let dir = workdir();
    let down = dir.path().join("down");
    std::fs::create_dir(&down).expect("mkdir");
    let runtime = || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime")
    };

    let first = runtime();
    first.block_on(async {
        let (engine, db) = engine(dir.path()).await;
        let id = connection(&engine, &live, "live-restart", "restart").await;
        connect_trusting(&engine, id).await;
        let job = transfer(
            &engine,
            vec![sftp("live-restart", "/home/tester/big.bin")],
            down.display().to_string(),
            JobOptions::default(),
            false,
        )
        .await;
        under_way(&engine, job, live.big).await;
        std::mem::forget(engine);
        std::mem::forget(db);
    });
    first.shutdown_background();
    assert!(down.join("big.bin.rcpart").exists(), "a part file is left");

    runtime().block_on(async {
        let (engine, _db) = engine(dir.path()).await;
        let restored = engine.restore_transfers().await.expect("restored");
        assert_eq!(restored.len(), 1, "{restored:?}");
        let job = &restored[0];
        assert_eq!(job.state, JobState::Paused, "{job:?}");
        assert!(job.resumable);
        engine
            .dispatch(Command::OpsResume { job: job.id })
            .await
            .expect("resumed");
        completed(&finished(&engine, job.id, LONG).await);
    });
    came_down_whole(&live, "restart", &down);
}
