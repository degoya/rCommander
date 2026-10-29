//! Resuming within one run (RC-060-07): a transfer whose connection breaks keeps its part file,
//! retries and continues from where the part ends - byte for byte the source, never spliced - and
//! a part an earlier copy left is offered to continue.

mod support;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{
    Answer, Backend, ConflictChoice, JobChange, JobKind, JobOptions, JobSpec, JobState, Question,
    RetryPolicy, TAIL_CHECK,
};
use rc_vfs::{BackendId, MemoryFs, VfsPath};
use support::{
    Harness, exists,
    flaky::{Flaky, wait_until},
    mem, payload, read, write,
};

const SIZE: usize = 2 * 1024 * 1024;
const HALF: u64 = 1024 * 1024 + 123;
const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

/// A harness with quick retries, `Memory(2)` holding `/home/big.bin`, and a flaky server.
async fn start(attempts: u32, delay: Duration) -> (Harness, MemoryFs, Flaky, Vec<u8>) {
    let harness = Harness::start();
    harness.queue.set_retry(RetryPolicy { attempts, delay });
    let local = harness.memory(2);
    let server = Flaky::new(MemoryFs::new());
    harness
        .queue
        .register(SERVER, Backend::new(Arc::new(server.clone())));
    let data = payload(SIZE, 7);
    write(&local, "/home/big.bin", &data).await;
    (harness, local, server, data)
}

fn upload(options: JobOptions) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![mem(2, "/home/big.bin")],
            target: on_server("/up"),
        },
        options,
    }
}

fn quick() -> Duration {
    Duration::from_millis(5)
}

#[tokio::test]
async fn an_upload_whose_connection_breaks_continues_from_the_offset() {
    let (harness, _local, server, data) = start(3, quick()).await;
    {
        let mut link = server.link();
        link.break_writes_after = Some(HALF);
        link.write_breaks = 1;
    }
    let job = harness.run(upload(JobOptions::default())).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert!(job.failures.is_empty());
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
    assert!(!exists(&server.fs, "/up/big.bin.rcpart").await);
    assert_eq!(
        server.link().written,
        SIZE as u64,
        "the second attempt wrote only what was missing"
    );
    assert!(!harness.changes(job.id).contains(&JobChange::Paused));
}

#[tokio::test]
async fn a_download_whose_connection_breaks_continues_from_the_offset() {
    let (harness, local, server, data) = start(3, quick()).await;
    write(&server.fs, "/srv/big.bin", &data).await;
    {
        let mut link = server.link();
        link.break_reads_after = Some(HALF);
        link.read_breaks = 1;
    }
    let job = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![on_server("/srv/big.bin")],
                target: mem(2, "/down"),
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&local, "/down/big.bin").await, data);
    assert!(!exists(&local, "/down/big.bin.rcpart").await);
    assert_eq!(
        server.link().read,
        SIZE as u64 + TAIL_CHECK,
        "the rest and the compared tail, never the whole file again"
    );
}

#[tokio::test]
async fn a_server_without_offset_reads_starts_the_file_over() {
    let (harness, local, server, data) = start(3, quick()).await;
    write(&server.fs, "/srv/big.bin", &data).await;
    {
        let mut link = server.link();
        link.break_reads_after = Some(HALF);
        link.read_breaks = 1;
        link.offsets = false;
    }
    let job = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![on_server("/srv/big.bin")],
                target: mem(2, "/down"),
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&local, "/down/big.bin").await, data, "never spliced");
    assert!(server.link().read >= HALF + SIZE as u64, "read again whole");
}

#[tokio::test]
async fn spent_retries_pause_the_job_as_resumable_until_it_is_resumed() {
    let (harness, _local, server, data) = start(1, quick()).await;
    {
        let mut link = server.link();
        link.break_writes_after = Some(256 * 1024);
        link.write_breaks = 3;
    }
    let id = harness
        .queue
        .submit(upload(JobOptions::default()))
        .expect("accepted")
        .id;
    let queue = harness.queue.clone();
    wait_until("the job pauses", || {
        queue.job(id).is_ok_and(|job| job.state == JobState::Paused)
    })
    .await;
    let paused = harness.queue.job(id).expect("known");
    assert!(paused.resumable);
    // Why it waits (RC-080-04): the lost connection, on the file it was copying.
    let [why] = paused.failures.as_slice() else {
        panic!("one failure while paused: {:?}", paused.failures);
    };
    assert!(why.path.path.ends_with("big.bin"), "{why:?}");
    assert!(!why.message.is_empty(), "{why:?}");
    assert!(
        exists(&server.fs, "/up/big.bin.rcpart").await,
        "the part stays"
    );
    server.link().write_breaks = 0;
    harness.queue.resume(id).expect("resume");
    let job = harness.finish(id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert!(
        job.failures.is_empty(),
        "gone once resumed: {:?}",
        job.failures
    );
    assert!(!job.resumable);
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
    assert_eq!(server.link().written, SIZE as u64);
}

#[tokio::test]
async fn a_cancel_while_waiting_for_a_retry_removes_the_part() {
    let (harness, _local, server, _data) = start(3, Duration::from_secs(30)).await;
    {
        let mut link = server.link();
        link.break_writes_after = Some(256 * 1024);
        link.write_breaks = 1;
    }
    let id = harness
        .queue
        .submit(upload(JobOptions::default()))
        .expect("accepted")
        .id;
    let link = Arc::clone(&server.link);
    wait_until("the link broke", || {
        link.lock().expect("link").write_breaks == 0
    })
    .await;
    harness.queue.cancel(id).expect("cancel");
    let job = harness.finish(id).await;
    assert_eq!(job.state, JobState::Cancelled);
    assert!(!exists(&server.fs, "/up/big.bin.rcpart").await);
    assert!(!exists(&server.fs, "/up/big.bin").await);
}

#[tokio::test]
async fn a_part_found_is_offered_to_resume_and_continued() {
    let (harness, _local, server, data) = start(3, quick()).await;
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF as usize]).await;
    let offered = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&offered);
    harness.on_event(move |queue, event| {
        if let JobChange::Question {
            question: Question::Conflict {
                target, resumable, ..
            },
        } = &event.change
        {
            *seen.lock().expect("seen") = Some((target.path.path.clone(), *resumable));
            let answer = Answer::Conflict {
                choice: ConflictChoice::Resume,
                for_all: false,
            };
            queue.answer(event.job, answer).expect("answer");
        }
    });
    let job = harness.run(upload(JobOptions::default())).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    let offered = offered.lock().expect("offered").clone();
    assert_eq!(offered, Some(("/up/big.bin.rcpart".into(), true)));
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
    assert_eq!(server.link().written, SIZE as u64 - HALF, "only the rest");
    // The file's bar continues from the part's end (RC-010-07).
    let file: Vec<u64> = harness
        .changes(job.id)
        .iter()
        .filter_map(|change| match change {
            JobChange::Progress { progress } if progress.file_bytes_done > 0 => {
                Some(progress.file_bytes_done)
            }
            _ => None,
        })
        .collect();
    assert_eq!(file.first(), Some(&HALF), "{file:?}");
    assert_eq!(file.last(), Some(&(SIZE as u64)), "{file:?}");
}

#[tokio::test]
async fn a_resumed_file_that_verifies_is_checked_whole() {
    let (harness, _local, server, data) = start(3, quick()).await;
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF as usize]).await;
    let options = JobOptions {
        on_conflict: Some(ConflictChoice::Resume),
        verify: true,
        ..JobOptions::default()
    };
    let job = harness.run(upload(options)).await;
    assert_eq!(job.state, JobState::Completed);
    assert!(job.failures.is_empty(), "{:?}", job.failures);
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
}

#[tokio::test]
async fn a_part_that_is_not_the_sources_starts_over() {
    let (harness, _local, server, data) = start(3, quick()).await;
    // Another file's bytes: the tail does not match.
    write(&server.fs, "/up/big.bin.rcpart", &vec![0_u8; HALF as usize]).await;
    let options = JobOptions {
        on_conflict: Some(ConflictChoice::Resume),
        ..JobOptions::default()
    };
    let job = harness.run(upload(options)).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&server.fs, "/up/big.bin").await, data, "never spliced");
    assert_eq!(server.link().written, SIZE as u64);
}

#[tokio::test]
async fn a_part_larger_than_the_source_is_not_resumed() {
    let (harness, _local, server, _data) = start(3, quick()).await;
    write(&server.fs, "/up/big.bin.rcpart", &payload(SIZE + 10, 1)).await;
    let options = JobOptions {
        on_conflict: Some(ConflictChoice::Resume),
        ..JobOptions::default()
    };
    let job = harness.run(upload(options)).await;
    assert_eq!(job.state, JobState::Completed);
    assert_eq!(job.skipped, 1);
    assert!(!exists(&server.fs, "/up/big.bin").await);
    assert!(exists(&server.fs, "/up/big.bin.rcpart").await);
}
