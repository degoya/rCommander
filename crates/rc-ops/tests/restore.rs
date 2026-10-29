//! Transfers across a restart (RC-060-07): a kept job and its marks in the store, a job put back
//! paused that continues its part only while the source is unchanged, files already complete
//! counted as done - a move removes their sources -, a discarded job, and a server opened again
//! through the reconnect hook.

mod support;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use rc_ops::{
    Backend, DeleteMode, FileMark, JobKind, JobOptions, JobSpec, JobState, OpsError, Reconnect,
    RetryPolicy, SavedJob, TransferStore,
};
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsPath};
use support::{
    Harness, exists,
    flaky::{Flaky, wait_until},
    mem, payload, read, set_mtime, write,
};

const SIZE: usize = 2 * 1024 * 1024;
const HALF: usize = 1024 * 1024 + 123;
const SERVER: BackendId = BackendId::Sftp { connection_id: 4 };

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

/// A store that records what it is told; every job it saves gets record 41.
#[derive(Default)]
struct Recording {
    saved: Mutex<Vec<JobSpec>>,
    marks: Mutex<Vec<(i64, FileMark)>>,
    forgotten: Mutex<Vec<i64>>,
}

#[async_trait]
impl TransferStore for Recording {
    async fn save(&self, spec: &JobSpec) -> Option<i64> {
        self.saved.lock().expect("saved").push(spec.clone());
        Some(41)
    }
    async fn mark(&self, record: i64, mark: &FileMark) {
        self.marks
            .lock()
            .expect("marks")
            .push((record, mark.clone()));
    }
    async fn forget(&self, record: i64) {
        self.forgotten.lock().expect("forgotten").push(record);
    }
}

/// A harness with a recording store, `Memory(2)` holding `/home/big.bin`, and a server.
async fn start() -> (Harness, Arc<Recording>, MemoryFs, Flaky, Vec<u8>) {
    let harness = Harness::start();
    harness.queue.set_retry(RetryPolicy {
        attempts: 1,
        delay: Duration::from_millis(5),
    });
    let store = Arc::new(Recording::default());
    harness
        .queue
        .set_store(Arc::clone(&store) as Arc<dyn TransferStore>);
    let local = harness.memory(2);
    let server = Flaky::new(MemoryFs::new());
    harness
        .queue
        .register(SERVER, Backend::new(Arc::new(server.clone())));
    let data = payload(SIZE, 9);
    write(&local, "/home/big.bin", &data).await;
    set_mtime(&local, "/home/big.bin", 1_700_000_000).await;
    (harness, store, local, server, data)
}

fn upload(sources: &[&str]) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: sources.iter().map(|path| mem(2, path)).collect(),
            target: on_server("/up"),
        },
        options: JobOptions::default(),
    }
}

/// The mark of `/home/big.bin` going to `/up/big.bin`, as the job made it before the restart.
fn mark(size: u64) -> FileMark {
    FileMark {
        source: PathBuf::from("/home/big.bin"),
        target: PathBuf::from("/up/big.bin"),
        size,
        modified_ns: Some(1_700_000_000 * 1_000_000_000),
        replace: false,
    }
}

#[tokio::test]
async fn a_transfer_is_kept_marked_and_forgotten_once_complete() {
    let (harness, store, local, _server, _data) = start().await;
    let job = harness.run(upload(&["/home/big.bin"])).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(store.saved.lock().expect("saved").len(), 1);
    let marks = store.marks.lock().expect("marks").clone();
    assert_eq!(marks, vec![(41, mark(SIZE as u64))]);
    assert_eq!(*store.forgotten.lock().expect("forgotten"), vec![41]);

    // A copy from disk to disk is not kept.
    let _copies = harness.memory(3);
    write(&local, "/home/small.txt", b"x").await;
    let job = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![mem(2, "/home/small.txt")],
                target: mem(3, "/copy"),
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(job.state, JobState::Completed);
    assert_eq!(store.saved.lock().expect("saved").len(), 1);
}

#[tokio::test]
async fn a_restored_job_waits_paused_and_continues_its_part() {
    let (harness, store, _local, server, data) = start().await;
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF]).await;
    let restored = harness
        .queue
        .restore(SavedJob {
            record: 9,
            spec: upload(&["/home/big.bin"]),
            mark: Some(mark(SIZE as u64)),
        })
        .expect("restored");
    assert_eq!(restored.state, JobState::Paused);
    assert!(restored.resumable);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        server.link().written,
        0,
        "nothing runs before it is resumed"
    );
    harness.queue.resume(restored.id).expect("resume");
    let job = harness.finish(restored.id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
    assert!(!exists(&server.fs, "/up/big.bin.rcpart").await);
    assert_eq!(server.link().written, (SIZE - HALF) as u64, "only the rest");
    assert!(
        store.saved.lock().expect("saved").is_empty(),
        "kept already"
    );
    assert_eq!(*store.forgotten.lock().expect("forgotten"), vec![9]);
}

#[tokio::test]
async fn a_changed_source_starts_the_file_over() {
    let (harness, _store, _local, server, data) = start().await;
    // What the part holds came from the source as it was: another size then.
    write(&server.fs, "/up/big.bin.rcpart", &vec![0_u8; HALF]).await;
    let restored = harness
        .queue
        .restore(SavedJob {
            record: 9,
            spec: upload(&["/home/big.bin"]),
            mark: Some(mark(SIZE as u64 + 1)),
        })
        .expect("restored");
    harness.queue.resume(restored.id).expect("resume");
    let job = harness.finish(restored.id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&server.fs, "/up/big.bin").await, data, "never spliced");
    assert_eq!(server.link().written, SIZE as u64);
}

#[tokio::test]
async fn files_already_complete_count_as_done_without_a_question() {
    let (harness, _store, local, server, data) = start().await;
    write(&local, "/home/a.txt", b"alpha").await;
    set_mtime(&local, "/home/a.txt", 1_600_000_000).await;
    write(&server.fs, "/up/a.txt", b"alpha").await;
    set_mtime(&server.fs, "/up/a.txt", 1_600_000_000).await;
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF]).await;
    let restored = harness
        .queue
        .restore(SavedJob {
            record: 9,
            spec: upload(&["/home/a.txt", "/home/big.bin"]),
            mark: Some(mark(SIZE as u64)),
        })
        .expect("restored");
    harness.queue.resume(restored.id).expect("resume");
    let job = harness.finish(restored.id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(job.skipped, 1, "a.txt was there already");
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
}

/// RC-080-37: a restored move that finds a file complete at its target still removes the source.
#[tokio::test]
async fn a_restored_move_removes_the_sources_of_files_already_complete() {
    let (harness, _store, local, server, data) = start().await;
    write(&local, "/home/a.txt", b"alpha").await;
    set_mtime(&local, "/home/a.txt", 1_600_000_000).await;
    write(&server.fs, "/up/a.txt", b"alpha").await;
    set_mtime(&server.fs, "/up/a.txt", 1_600_000_000).await;
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF]).await;
    let restored = harness
        .queue
        .restore(SavedJob {
            record: 9,
            spec: JobSpec {
                kind: JobKind::Move {
                    sources: vec![mem(2, "/home/a.txt"), mem(2, "/home/big.bin")],
                    target: on_server("/up"),
                },
                options: JobOptions::default(),
            },
            mark: Some(mark(SIZE as u64)),
        })
        .expect("restored");
    harness.queue.resume(restored.id).expect("resume");
    let job = harness.finish(restored.id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert!(
        !exists(&local, "/home/a.txt").await,
        "the complete file's source"
    );
    assert!(
        !exists(&local, "/home/big.bin").await,
        "the continued file's source"
    );
    assert_eq!(read(&server.fs, "/up/a.txt").await, b"alpha");
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
}

#[tokio::test]
async fn a_cancelled_restored_job_is_discarded() {
    let (harness, store, _local, server, data) = start().await;
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF]).await;
    let restored = harness
        .queue
        .restore(SavedJob {
            record: 9,
            spec: upload(&["/home/big.bin"]),
            mark: Some(mark(SIZE as u64)),
        })
        .expect("restored");
    harness.queue.cancel(restored.id).expect("cancel");
    let job = harness.finish(restored.id).await;
    assert_eq!(job.state, JobState::Cancelled);
    assert!(!exists(&server.fs, "/up/big.bin.rcpart").await, "discarded");
    assert_eq!(*store.forgotten.lock().expect("forgotten"), vec![9]);
}

/// Opens the server on demand, or fails while `fail` holds.
struct Opener {
    server: Flaky,
    fail: Mutex<bool>,
}

#[async_trait]
impl Reconnect for Opener {
    async fn reconnect(&self, _backend: &BackendId) -> Result<Arc<dyn Vfs>, OpsError> {
        if *self.fail.lock().expect("fail") {
            return Err(OpsError::BackendUnavailable);
        }
        Ok(Arc::new(self.server.clone()))
    }
}

#[tokio::test]
async fn a_restored_job_opens_its_server_when_resumed() {
    let (harness, _store, _local, server, data) = start().await;
    harness.queue.unregister(&SERVER);
    write(&server.fs, "/up/big.bin.rcpart", &data[..HALF]).await;
    let opener = Arc::new(Opener {
        server: server.clone(),
        fail: Mutex::new(true),
    });
    harness
        .queue
        .set_reconnect(Arc::clone(&opener) as Arc<dyn Reconnect>);
    let restored = harness
        .queue
        .restore(SavedJob {
            record: 9,
            spec: upload(&["/home/big.bin"]),
            mark: Some(mark(SIZE as u64)),
        })
        .expect("restored");
    let id = restored.id;
    // The server cannot be had: the job stays paused, the reason listed.
    harness.queue.resume(id).expect("resume");
    let queue = harness.queue.clone();
    wait_until("the job pauses again with a failure", || {
        queue
            .job(id)
            .is_ok_and(|job| job.state == JobState::Paused && !job.failures.is_empty())
    })
    .await;
    *opener.fail.lock().expect("fail") = false;
    harness.queue.resume(id).expect("resume again");
    let job = harness.finish(id).await;
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert_eq!(read(&server.fs, "/up/big.bin").await, data);
}

#[tokio::test]
async fn only_a_transfer_is_restored() {
    let (harness, _store, _local, _server, _data) = start().await;
    let refused = harness.queue.restore(SavedJob {
        record: 1,
        spec: JobSpec {
            kind: JobKind::Delete {
                sources: vec![on_server("/up/a")],
                mode: DeleteMode::Permanent,
            },
            options: JobOptions::default(),
        },
        mark: None,
    });
    assert_eq!(
        refused.map(|job| job.id).map_err(|error| error.code()),
        Err("unsupported")
    );
}
