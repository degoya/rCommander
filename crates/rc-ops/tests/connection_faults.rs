//! Connections that break in the middle (RC-080-04), against the real servers of this process: an
//! SFTP server that goes silent during an upload - as a paused container or a pulled cable does -,
//! one that is silent before a download lists its folder, and an FTP server that stops sending
//! in the middle of a file. Each ends in a state with a message: a transfer pauses resumable with
//! its part kept and nothing under the target's name, a listing fails with its code. Nothing
//! panics, and a cancel still ends the job.

mod support;

use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rc_ops::{
    Backend, ErrorPolicy, JobChange, JobId, JobKind, JobOptions, JobSnapshot, JobSpec, JobState,
    Queue, RetryPolicy,
};
use rc_remote::{ftp::test_server, sftp::SftpLimits};
use rc_vfs::{BackendId, VfsPath};
use support::{
    Harness, payload,
    servers::{Ftp, Sftp},
};

const SFTP: BackendId = BackendId::Sftp { connection_id: 1 };
const FTP: BackendId = BackendId::Ftp { connection_id: 2 };
const BIG: usize = 1 << 20;

fn at(backend: &BackendId, path: &str) -> VfsPath {
    VfsPath {
        backend: backend.clone(),
        path: path.into(),
    }
}

/// A queue with this machine registered that pauses at the first lost connection.
fn harness() -> Harness {
    let harness = Harness::start();
    harness.local();
    harness.queue.set_retry(RetryPolicy {
        attempts: 0,
        delay: Duration::from_millis(5),
    });
    harness
}

/// Waits until `job` is paused and resumable after a lost connection, at most 20 seconds.
async fn paused(queue: &Queue, job: JobId) -> JobSnapshot {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let snapshot = queue.job(job).expect("known");
        if snapshot.state == JobState::Paused && snapshot.resumable {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "the job never paused: {snapshot:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The codes of a job's failures.
fn codes(job: &JobSnapshot) -> Vec<&str> {
    job.failures
        .iter()
        .map(|failure| failure.code.as_str())
        .collect()
}

/// A code a lost connection or a silent server is reported with.
fn is_lost(code: &str) -> bool {
    matches!(
        code,
        "sftp_timeout" | "connection_lost" | "io" | "ftp_timeout"
    )
}

#[tokio::test]
async fn an_upload_whose_server_goes_silent_pauses_resumable_and_keeps_its_name_free() {
    let sftp = Arc::new(
        Sftp::start_with(SftpLimits {
            request_timeout: Duration::from_secs(1),
            ..SftpLimits::default()
        })
        .await,
    );
    let harness = harness();
    harness.queue.register(SFTP, Backend::new(sftp.vfs.clone()));
    let here = tempfile::tempdir().expect("local directory");
    let source = here.path().join("big.bin");
    fs::write(&source, payload(BIG, 7)).expect("write");
    let frozen = Arc::new(AtomicBool::new(false));
    {
        let (sftp, frozen) = (Arc::clone(&sftp), Arc::clone(&frozen));
        harness.on_event(move |_queue, event| {
            if let JobChange::Progress { progress } = &event.change
                && progress.bytes_done > 0
                && !frozen.swap(true, Ordering::SeqCst)
            {
                sftp.server.freeze();
            }
        });
    }
    let id = harness
        .queue
        .submit(JobSpec {
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(&source)],
                target: at(&SFTP, "/up"),
            },
            options: JobOptions::default(),
        })
        .expect("accepted")
        .id;
    let job = paused(&harness.queue, id).await;
    assert!(frozen.load(Ordering::SeqCst), "the server went silent");
    assert!(
        codes(&job).iter().all(|code| is_lost(code)),
        "{:?}",
        job.failures
    );
    assert!(
        !sftp.on_disk("/up/big.bin").exists(),
        "nothing under the target's name"
    );
    harness.queue.cancel(id).expect("cancel");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Cancelled);
    assert!(source.exists(), "the source stays");
}

#[tokio::test]
async fn a_download_from_a_silent_server_fails_with_its_code() {
    let sftp = Sftp::start_with(SftpLimits {
        request_timeout: Duration::from_secs(1),
        ..SftpLimits::default()
    })
    .await;
    fs::create_dir_all(sftp.on_disk("/down")).expect("mkdir");
    fs::write(sftp.on_disk("/down/a.txt"), b"alpha").expect("write");
    let harness = harness();
    harness.queue.register(SFTP, Backend::new(sftp.vfs.clone()));
    sftp.server.freeze();
    let here = tempfile::tempdir().expect("local directory");
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![at(&SFTP, "/down")],
                target: VfsPath::local(here.path()),
            },
            options: JobOptions {
                on_error: ErrorPolicy::Skip,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let codes = codes(&done);
    assert!(
        !codes.is_empty() && codes.iter().all(|code| is_lost(code)),
        "{:?}",
        done.failures
    );
    assert!(!here.path().join("down").join("a.txt").exists());
}

#[tokio::test]
async fn a_download_whose_ftp_server_stops_sending_pauses_with_its_part() {
    let ftp = Ftp::start_with(
        test_server::Options {
            stall: true,
            ..test_server::Options::default()
        },
        Duration::from_secs(1),
    )
    .await;
    fs::write(ftp.on_disk("slow.bin"), payload(64 * 1024, 3)).expect("write");
    let harness = harness();
    harness
        .queue
        .register(FTP, Backend::new(Arc::new(ftp.vfs.clone())));
    let here = tempfile::tempdir().expect("local directory");
    let id = harness
        .queue
        .submit(JobSpec {
            kind: JobKind::Copy {
                sources: vec![at(&FTP, "/slow.bin")],
                target: VfsPath::local(here.path()),
            },
            options: JobOptions::default(),
        })
        .expect("accepted")
        .id;
    let job = paused(&harness.queue, id).await;
    assert!(
        codes(&job).iter().all(|code| is_lost(code)),
        "{:?}",
        job.failures
    );
    assert!(
        !here.path().join("slow.bin").exists(),
        "nothing under the target's name"
    );
    assert!(
        here.path().join("slow.bin.rcpart").exists(),
        "the part waits to be continued"
    );
    harness.queue.cancel(id).expect("cancel");
    let done = harness.finish(id).await;
    assert_eq!(done.state, JobState::Cancelled);
    assert!(!here.path().join("slow.bin.rcpart").exists(), "removed");
}
