//! F5 and F6 between two servers and within one: byte for byte, the source of a move gone only
//! after the copy, and a target that refuses the write named as the target.

use std::time::Duration;

use rc_engine::{ErrorPolicy, JobOptions, JobState};
use rc_vfs::BackendId;

use crate::live::{
    completed, connect_trusting, connection, engine, finished, sftp, transfer, workdir,
};
use crate::live_or_skip;

const LONG: Duration = Duration::from_secs(240);

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn copies_and_moves_between_two_servers_and_within_one_are_byte_for_byte() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let a = connection(&engine, &live, "live-a", "a").await;
    let b = connection(&engine, &live, "live-b", "b").await;
    connect_trusting(&engine, a).await;
    connect_trusting(&engine, b).await;
    let digest = live.remote_sha256("a", "/home/tester/big.bin");
    let on_b = live.remote_dir("b", "t-s2s");
    let within = live.remote_dir("a", "t-within");
    let moved = live.remote_dir("a", "t-s2s-moved");

    let across = transfer(
        &engine,
        vec![sftp("live-a", "/home/tester/big.bin")],
        sftp("live-b", &on_b),
        JobOptions::default(),
        false,
    )
    .await;
    completed(&finished(&engine, across, LONG).await);
    assert_eq!(live.remote_sha256("b", &format!("{on_b}/big.bin")), digest);

    let same = transfer(
        &engine,
        vec![sftp("live-a", "/home/tester/big.bin")],
        sftp("live-a", &within),
        JobOptions::default(),
        false,
    )
    .await;
    completed(&finished(&engine, same, LONG).await);
    assert_eq!(
        live.remote_sha256("a", &format!("{within}/big.bin")),
        digest
    );

    let back = transfer(
        &engine,
        vec![sftp("live-b", &format!("{on_b}/big.bin"))],
        sftp("live-a", &moved),
        JobOptions::default(),
        true,
    )
    .await;
    completed(&finished(&engine, back, LONG).await);
    assert_eq!(live.remote_sha256("a", &format!("{moved}/big.bin")), digest);
    assert!(
        !live.remote_exists("b", &format!("{on_b}/big.bin")),
        "the move's source is gone"
    );
    assert_eq!(live.remote_names("a", &moved), ["big.bin"], "no part file");
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_target_that_refuses_the_write_is_named_as_the_target() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let a = connection(&engine, &live, "live-a", "a").await;
    let b = connection(&engine, &live, "live-b", "b").await;
    connect_trusting(&engine, a).await;
    connect_trusting(&engine, b).await;

    let refused = transfer(
        &engine,
        vec![sftp("live-a", "/home/tester/docs/readme.txt")],
        sftp("live-b", "/srv/readonly"),
        JobOptions {
            on_error: ErrorPolicy::Cancel,
            ..JobOptions::default()
        },
        false,
    )
    .await;
    let job = finished(&engine, refused, LONG).await;
    assert_eq!(job.state, JobState::Failed, "{job:?}");
    let failure = job.failures.first().expect("a failure");
    eprintln!("failure: {failure:?}");
    let target = failure.target.as_deref().expect("the target is named");
    assert_eq!(
        target.backend,
        BackendId::Sftp { connection_id: b.0 },
        "{failure:?}"
    );
    assert!(target.path.starts_with("/srv/readonly"), "{failure:?}");
}
