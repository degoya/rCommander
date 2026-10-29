//! F5 and F6 between the local disk and a server: a big file byte for byte in both directions,
//! `<name>.rcpart` on the server until the upload is complete, the time stamp kept, a cancel that
//! leaves no file under the real name, a move that removes the source only after the copy, and a
//! copy into a quick connection (Ctrl+N, not saved).

use std::time::{Duration, SystemTime};

use rc_engine::{Command, JobOptions, JobState, Outcome, QuickConnectRequest, SecretParam};

use crate::live::{
    completed, connect_trusting, connection, engine, finished, local_mtime, local_sha256,
    random_file, sftp, transfer, trust_open_question, under_way, workdir,
};
use crate::live_or_skip;

const LONG: Duration = Duration::from_secs(240);
const WHEN: u64 = 1_600_000_000;

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_big_file_goes_up_as_rcpart_and_comes_back_byte_for_byte_with_its_time() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let mine = live.remote_dir("a", "t-updown");

    let source = dir.path().join("big.bin");
    random_file(&source, live.big, 11);
    std::fs::File::options()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(WHEN)))
        .expect("set mtime");
    let digest = local_sha256(&source);

    let up = transfer(
        &engine,
        vec![source.display().to_string()],
        sftp("live-a", &mine),
        JobOptions::default(),
        false,
    )
    .await;
    under_way(&engine, up, live.big).await;
    engine
        .dispatch(Command::OpsPause { job: up })
        .await
        .expect("paused");
    assert_eq!(
        live.remote_names("a", &mine),
        ["big.bin.rcpart"],
        "only the part file while the upload runs"
    );
    engine
        .dispatch(Command::OpsResume { job: up })
        .await
        .expect("resumed");
    completed(&finished(&engine, up, LONG).await);
    assert_eq!(live.remote_names("a", &mine), ["big.bin"]);
    assert_eq!(live.remote_sha256("a", &format!("{mine}/big.bin")), digest);
    let remote_time = live.ctl(&["exec", "a", &format!("stat -c %Y '{mine}/big.bin'")]);
    assert_eq!(remote_time.trim(), WHEN.to_string(), "the time stamp kept");

    let back = dir.path().join("back");
    std::fs::create_dir(&back).expect("mkdir");
    let down = transfer(
        &engine,
        vec![sftp("live-a", &format!("{mine}/big.bin"))],
        back.display().to_string(),
        JobOptions::default(),
        false,
    )
    .await;
    completed(&finished(&engine, down, LONG).await);
    assert_eq!(local_sha256(&back.join("big.bin")), digest, "byte for byte");
    assert_eq!(local_mtime(&back.join("big.bin")), WHEN);
    assert!(!back.join("big.bin.rcpart").exists());
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_cancelled_upload_leaves_no_file_under_the_real_name() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let mine = live.remote_dir("a", "t-cancel");
    let source = dir.path().join("big.bin");
    random_file(&source, live.big, 12);

    let up = transfer(
        &engine,
        vec![source.display().to_string()],
        sftp("live-a", &mine),
        JobOptions::default(),
        false,
    )
    .await;
    under_way(&engine, up, live.big).await;
    engine
        .dispatch(Command::OpsCancel { job: up })
        .await
        .expect("cancelled");
    let job = finished(&engine, up, LONG).await;
    assert_eq!(job.state, JobState::Cancelled, "{:?}", job.failures);
    let left = live.remote_names("a", &mine);
    assert!(!left.contains(&"big.bin".to_owned()), "{left:?}");
    assert!(
        left.is_empty(),
        "the part file is removed as well: {left:?}"
    );
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_move_removes_the_source_only_after_the_copy() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let mine = live.remote_dir("a", "t-move");
    let source = dir.path().join("big.bin");
    random_file(&source, live.big, 13);
    let digest = local_sha256(&source);

    let moving = transfer(
        &engine,
        vec![source.display().to_string()],
        sftp("live-a", &mine),
        JobOptions::default(),
        true,
    )
    .await;
    under_way(&engine, moving, live.big).await;
    engine
        .dispatch(Command::OpsPause { job: moving })
        .await
        .expect("paused");
    assert!(source.exists(), "the source stays while the copy runs");
    engine
        .dispatch(Command::OpsResume { job: moving })
        .await
        .expect("resumed");
    completed(&finished(&engine, moving, LONG).await);
    assert!(!source.exists(), "the source went after the copy");
    assert_eq!(live.remote_sha256("a", &format!("{mine}/big.bin")), digest);
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_quick_connection_that_is_not_saved_takes_a_copy_both_ways() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let mine = live.remote_dir("a", "t-quick");
    let request = QuickConnectRequest {
        panel: Some(rc_engine::PanelId::Right),
        host: live.host.clone(),
        port: Some(live.port("a")),
        user: live.user.clone(),
        password: Some(SecretParam::from(live.password.as_str())),
        ..QuickConnectRequest::default()
    };
    let first = engine
        .dispatch(Command::QuickConnect(request.clone()))
        .await
        .expect_err("the key is asked about first");
    assert_eq!(first.code(), "host_key_unknown", "{first}");
    trust_open_question(&engine).await;
    let shown = engine.dispatch(Command::QuickConnect(request)).await;
    assert!(matches!(shown, Ok(Outcome::Panel(_))), "{shown:?}");

    let source = dir.path().join("small.bin");
    random_file(&source, 5 * 1024 * 1024, 14);
    let digest = local_sha256(&source);
    let named = format!("sftp://{}@{}:{}", live.user, live.host, live.port("a"));
    let up = transfer(
        &engine,
        vec![source.display().to_string()],
        format!("{named}{mine}"),
        JobOptions::default(),
        false,
    )
    .await;
    completed(&finished(&engine, up, LONG).await);
    assert_eq!(
        live.remote_sha256("a", &format!("{mine}/small.bin")),
        digest
    );

    let back = dir.path().join("back");
    std::fs::create_dir(&back).expect("mkdir");
    let down = transfer(
        &engine,
        vec![format!("{named}{mine}/small.bin")],
        back.display().to_string(),
        JobOptions::default(),
        false,
    )
    .await;
    completed(&finished(&engine, down, LONG).await);
    assert_eq!(local_sha256(&back.join("small.bin")), digest);
}
