//! A copy between two servers whose one side fails mid-stream (RC-060-06): both streams end,
//! the failure says which side it was, and nothing is left under the target's name. A lost
//! connection keeps the part and continues it (RC-060-07), from either side.

mod support;

use std::{io, sync::Arc, time::Duration};

use rc_ops::{
    Backend, ErrorPolicy, JobChange, JobKind, JobOptions, JobSpec, JobState, RetryPolicy,
};
use rc_vfs::{BackendId, MemoryFs, VfsPath};
use support::{
    Harness, exists,
    flaky::{Flaky, wait_until},
    payload, read, write,
};

const SIZE: usize = 2 * 1024 * 1024;
const HALF: u64 = 1024 * 1024 + 123;
const FROM: BackendId = BackendId::Sftp { connection_id: 1 };
const TO: BackendId = BackendId::Ftp { connection_id: 2 };

fn at(backend: &BackendId, path: &str) -> VfsPath {
    VfsPath {
        backend: backend.clone(),
        path: path.into(),
    }
}

/// Two flaky servers, `/srv/big.bin` on the first, retries as given.
async fn start(attempts: u32) -> (Harness, Flaky, Flaky, Vec<u8>) {
    let harness = Harness::start();
    harness.queue.set_retry(RetryPolicy {
        attempts,
        delay: Duration::from_millis(5),
    });
    let (from, to) = (Flaky::new(MemoryFs::new()), Flaky::new(MemoryFs::new()));
    harness
        .queue
        .register(FROM, Backend::new(Arc::new(from.clone())));
    harness
        .queue
        .register(TO, Backend::new(Arc::new(to.clone())));
    let data = payload(SIZE, 13);
    write(&from.fs, "/srv/big.bin", &data).await;
    (harness, from, to, data)
}

fn across(on_error: ErrorPolicy) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![at(&FROM, "/srv/big.bin")],
            target: at(&TO, "/in"),
        },
        options: JobOptions {
            on_error,
            ..JobOptions::default()
        },
    }
}

#[tokio::test]
async fn a_failing_target_ends_both_streams_and_names_the_target() {
    let (harness, from, to, _data) = start(0).await;
    {
        let mut link = to.link();
        link.break_writes_after = Some(HALF);
        link.write_breaks = 1;
        link.fault = Some(io::ErrorKind::PermissionDenied);
    }
    let job = harness.run(across(ErrorPolicy::Cancel)).await;
    assert_eq!(job.state, JobState::Failed, "{job:?}");
    let failure = &job.failures[0];
    assert_eq!(failure.path, at(&FROM, "/srv/big.bin"), "the file");
    assert_eq!(
        failure.target,
        Some(Box::new(at(&TO, "/in/big.bin.rcpart"))),
        "the side that failed"
    );
    assert!(
        !exists(&to.fs, "/in/big.bin").await,
        "nothing under the name"
    );
    assert!(!exists(&to.fs, "/in/big.bin.rcpart").await, "the part went");
    let read_ahead = from.link().read - to.link().written;
    assert!(
        read_ahead <= 5 * 4096,
        "the source stopped with the target: {read_ahead} bytes read ahead"
    );
}

/// A read-only target folder refuses the part file before a byte moves (RC-060-06): the
/// failure names the target, so the error card says where it failed.
#[tokio::test]
async fn a_target_that_refuses_the_part_file_is_named_as_the_side_that_failed() {
    let (harness, _from, to, _data) = start(0).await;
    to.link().refuse_writes = true;
    let job = harness.run(across(ErrorPolicy::Cancel)).await;
    assert_eq!(job.state, JobState::Failed, "{job:?}");
    let failure = &job.failures[0];
    assert_eq!(failure.code, "permission_denied");
    assert_eq!(failure.path, at(&FROM, "/srv/big.bin"), "the file");
    assert_eq!(
        failure.target,
        Some(Box::new(at(&TO, "/in/big.bin.rcpart"))),
        "the side that failed"
    );
    assert_eq!(to.link().written, 0, "nothing was written");
}

/// A target that takes the part but refuses to rename it: the target is named with the file's
/// own name, and nothing is left under either name.
#[tokio::test]
async fn a_target_that_refuses_the_final_rename_is_named_as_the_side_that_failed() {
    let (harness, _from, to, _data) = start(0).await;
    to.link().refuse_renames = true;
    let job = harness.run(across(ErrorPolicy::Cancel)).await;
    assert_eq!(job.state, JobState::Failed, "{job:?}");
    let failure = &job.failures[0];
    assert_eq!(failure.code, "permission_denied");
    assert_eq!(
        failure.target,
        Some(Box::new(at(&TO, "/in/big.bin"))),
        "the side that failed"
    );
    assert!(!exists(&to.fs, "/in/big.bin").await);
    assert!(!exists(&to.fs, "/in/big.bin.rcpart").await, "the part went");
}

#[tokio::test]
async fn a_failing_source_ends_both_streams_and_names_no_target() {
    let (harness, from, to, _data) = start(0).await;
    {
        let mut link = from.link();
        link.break_reads_after = Some(HALF);
        link.read_breaks = 1;
        link.fault = Some(io::ErrorKind::InvalidData);
    }
    let job = harness.run(across(ErrorPolicy::Cancel)).await;
    assert_eq!(job.state, JobState::Failed, "{job:?}");
    let failure = &job.failures[0];
    assert_eq!(failure.path, at(&FROM, "/srv/big.bin"));
    assert_eq!(failure.target, None, "the source failed, not the target");
    assert!(!exists(&to.fs, "/in/big.bin").await);
    assert!(!exists(&to.fs, "/in/big.bin.rcpart").await);
    let (read, written) = (from.link().read, to.link().written);
    assert!(read >= HALF);
    assert_eq!(written, read, "what was read before the break was written");
}

#[tokio::test]
async fn a_lost_target_connection_keeps_the_part_and_pauses_until_resumed() {
    let (harness, _from, to, data) = start(0).await;
    {
        let mut link = to.link();
        link.break_writes_after = Some(HALF);
        link.write_breaks = 1;
    }
    let id = harness
        .queue
        .submit(across(ErrorPolicy::Ask))
        .expect("accepted")
        .id;
    let queue = harness.queue.clone();
    wait_until("the job pauses as resumable", || {
        queue.job(id).is_ok_and(|job| job.resumable)
    })
    .await;
    assert!(
        !exists(&to.fs, "/in/big.bin").await,
        "nothing under the name"
    );
    assert!(exists(&to.fs, "/in/big.bin.rcpart").await, "the part stays");

    harness.queue.resume(id).expect("resume");
    let job = harness.finish(id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&to.fs, "/in/big.bin").await, data);
    assert!(!exists(&to.fs, "/in/big.bin.rcpart").await);
    assert!(harness.changes(id).contains(&JobChange::Paused));
}

#[tokio::test]
async fn a_lost_source_connection_continues_where_the_part_ends() {
    let (harness, from, to, data) = start(3).await;
    {
        let mut link = from.link();
        link.break_reads_after = Some(HALF);
        link.read_breaks = 1;
    }
    let job = harness.run(across(ErrorPolicy::Ask)).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&to.fs, "/in/big.bin").await, data, "never spliced");
    assert!(!exists(&to.fs, "/in/big.bin.rcpart").await);
    assert_eq!(
        to.link().written,
        SIZE as u64,
        "the second attempt wrote only what was missing"
    );
}

#[tokio::test]
async fn a_cancel_mid_stream_leaves_nothing_on_the_target() {
    let (harness, _from, to, _data) = start(0).await;
    harness.on_event(|queue, event| {
        if let JobChange::Progress { progress } = &event.change
            && progress.current.is_some()
            && progress.bytes_done > 0
        {
            let _ = queue.cancel(event.job);
        }
    });
    let job = harness.run(across(ErrorPolicy::Ask)).await;
    assert_eq!(job.state, JobState::Cancelled, "{job:?}");
    assert!(!exists(&to.fs, "/in/big.bin").await);
    assert!(!exists(&to.fs, "/in/big.bin.rcpart").await);
}
