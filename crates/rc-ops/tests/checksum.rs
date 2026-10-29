//! Checksum files, their check, split and combine as jobs of the queue (RC-020-05): the report,
//! the progress at the end and the file at work on the way (RC-010-07), a failure with its code,
//! pause and cancel.

mod support;

use std::{path::Path, sync::Arc, time::Duration};

use rc_checksum::{Algorithm, Layout, VerifyStatus};
use rc_ops::{Backend, JobChange, JobKind, JobOptions, JobReport, JobSnapshot, JobSpec, JobState};
use rc_vfs::{BackendId, MemoryFs, Vfs};
use support::{Harness, exists, mem, payload, read, slow::Slow, write};

const SIZE: usize = 300_000;

fn spec(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions::default(),
    }
}

fn split_job() -> JobKind {
    JobKind::Split {
        source: mem(1, "/d/disk.iso"),
        target: mem(1, "/parts"),
        part_size: 100_000,
    }
}

async fn setup() -> (Harness, rc_vfs::MemoryFs) {
    let harness = Harness::start();
    let fs = harness.memory(1);
    write(&fs, "/d/disk.iso", &payload(SIZE, 7)).await;
    fs.mkdir(Path::new("/parts"), true).await.expect("parts");
    fs.mkdir(Path::new("/back"), true).await.expect("back");
    (harness, fs)
}

fn completed(job: &JobSnapshot) -> &JobReport {
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    job.report.as_ref().expect("a completed job has its report")
}

#[tokio::test]
async fn create_verify_split_and_combine_run_as_jobs_with_their_reports() {
    let (harness, fs) = setup().await;
    let created = harness
        .run(spec(JobKind::ChecksumCreate {
            dir: mem(1, "/d"),
            names: vec!["disk.iso".into()],
            algorithm: Algorithm::Sha256,
            layout: Layout::PerFile,
        }))
        .await;
    let JobReport::ChecksumsCreated { report, omitted } = completed(&created) else {
        panic!("a create report");
    };
    assert_eq!(
        (report.written.clone(), *omitted),
        (vec!["disk.iso.sha256".to_owned()], 0)
    );
    let progress = &created.progress;
    assert_eq!(
        (
            progress.files_done,
            progress.files_total,
            progress.bytes_total
        ),
        (1, 1, SIZE as u64)
    );

    let verified = harness
        .run(spec(JobKind::ChecksumVerify {
            file: mem(1, "/d/disk.iso.sha256"),
        }))
        .await;
    let JobReport::ChecksumsVerified { report, .. } = completed(&verified) else {
        panic!("a check's report");
    };
    assert_eq!(report.entries[0].status, VerifyStatus::Ok);
    assert!(report.summary.all_ok());

    let split = harness.run(spec(split_job())).await;
    let JobReport::Split { report } = completed(&split) else {
        panic!("a split report");
    };
    assert_eq!(report.parts, vec!["disk.001", "disk.002", "disk.003"]);
    assert_eq!(split.progress.files_done, 3);

    let combined = harness
        .run(spec(JobKind::Combine {
            first: mem(1, "/parts/disk.crc"),
            target: mem(1, "/back"),
            name: None,
        }))
        .await;
    let JobReport::Combined { report } = completed(&combined) else {
        panic!("a combine report");
    };
    assert!(report.crc_checked);
    assert_eq!(read(&fs, "/back/disk.iso").await, payload(SIZE, 7));
}

#[tokio::test]
async fn a_failing_run_is_the_jobs_failure_with_its_code() {
    let (harness, fs) = setup().await;
    // The second part is missing: nothing is combined.
    harness.run(spec(split_job())).await;
    fs.remove_file(Path::new("/parts/disk.002"))
        .await
        .expect("removed");
    let job = harness
        .run(spec(JobKind::Combine {
            first: mem(1, "/parts/disk.001"),
            target: mem(1, "/back"),
            name: None,
        }))
        .await;
    assert_eq!(job.state, JobState::Failed);
    assert_eq!(job.failures[0].code, "combine_part_missing");
    assert!(job.report.is_none());
    assert!(!exists(&fs, "/back/disk.iso").await);
}

#[tokio::test]
async fn a_paused_job_waits_and_a_cancelled_one_writes_nothing() {
    let (harness, fs) = setup().await;
    harness.on_event(|queue, event| {
        if event.change == JobChange::Started {
            let _ = queue.pause(event.job);
        }
    });
    let id = harness
        .queue
        .submit(spec(split_job()))
        .expect("accepted")
        .id;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let held = harness.queue.job(id).expect("known");
    assert_eq!(held.state, JobState::Paused);
    assert!(!exists(&fs, "/parts/disk.001").await, "a paused job wrote");
    harness.queue.resume(id).expect("resumed");
    let finished = harness.finish(id).await;
    let JobReport::Split { report } = completed(&finished) else {
        panic!("a split report");
    };
    assert_eq!(report.parts.len(), 3);

    harness.on_event(|queue, event| {
        if event.change == JobChange::Started {
            let _ = queue.cancel(event.job);
        }
    });
    let job = harness
        .run(spec(JobKind::Split {
            source: mem(1, "/d/disk.iso"),
            target: mem(1, "/back"),
            part_size: 100_000,
        }))
        .await;
    assert_eq!(job.state, JobState::Cancelled);
    assert!(!exists(&fs, "/back/disk.001").await);
    assert!(!exists(&fs, "/back/disk.crc").await);
}

/// The file bytes of every progress change while `name` was at work, each change naming `size`
/// as its total.
fn file_bytes(harness: &Harness, job: &JobSnapshot, name: &str, size: u64) -> Vec<u64> {
    harness
        .changes(job.id)
        .iter()
        .filter_map(|change| match change {
            JobChange::Progress { progress } => progress
                .current
                .as_ref()
                .filter(|at| at.path == Path::new(name))
                .map(|_| {
                    assert_eq!(progress.file_bytes_total, size, "{progress:?}");
                    progress.file_bytes_done
                }),
            _ => None,
        })
        .collect()
}

/// The progress window's File bar (RC-010-07): a large file's hash moves its own bytes from below
/// its size up to it, in a create and in a check. Against a server of limited bandwidth on
/// tokio's paused clock, so the job is caught half way on every machine.
#[tokio::test(start_paused = true)]
async fn a_large_file_s_hash_moves_its_own_bytes_up_to_its_size() {
    const BIG: u64 = 1 << 20;
    let harness = Harness::start();
    let server = Slow {
        fs: MemoryFs::new(),
        rate: 2 << 20,
    };
    harness
        .queue
        .register(BackendId::Memory(3), Backend::new(Arc::new(server.clone())));
    write(&server.fs, "/d/big.iso", &payload(BIG as usize, 3)).await;
    let created = harness
        .run(spec(JobKind::ChecksumCreate {
            dir: mem(3, "/d"),
            names: vec!["big.iso".into()],
            algorithm: Algorithm::Sha256,
            layout: Layout::PerFile,
        }))
        .await;
    completed(&created);
    let verified = harness
        .run(spec(JobKind::ChecksumVerify {
            file: mem(3, "/d/big.iso.sha256"),
        }))
        .await;
    let JobReport::ChecksumsVerified { report, .. } = completed(&verified) else {
        panic!("a check's report");
    };
    assert!(report.summary.all_ok());
    for job in [&created, &verified] {
        let own = file_bytes(&harness, job, "/d/big.iso", BIG);
        assert!(
            own.iter().any(|done| (1..BIG).contains(done)),
            "caught half way: {own:?}"
        );
        assert!(own.windows(2).all(|pair| pair[0] <= pair[1]), "{own:?}");
        assert_eq!(own.last(), Some(&BIG), "{own:?}");
        assert_eq!(
            job.progress.file_bytes_total, 0,
            "nothing at work once over"
        );
    }
}
