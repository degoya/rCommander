//! A process killed with copies running (RC-080-04): the runtime that ran the queue is shut down
//! in the middle of three copies from disk to disk - no job ends, no cleanup runs, as when the
//! process is killed - and a new queue on a new runtime finds the three jobs in the store. No
//! file is incomplete under its target's name after the kill; resumed, each copy ends byte for
//! byte as its source.
//!
//! The store outlives the first runtime as the database outlives a process. The engine's own test
//! (`rc-engine`, `transfers_crash_tests.rs`) does the same over a database file opened again.

mod support;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use rc_ops::{
    Backend, EventSink, FileMark, JobKind, JobOptions, JobSpec, JobState, MARK_MIN_BYTES, Queue,
    QueueConfig, SavedJob, Scheduling, TransferStore,
};
use rc_vfs::{BackendId, LocalFs, VfsPath, testing::Trickle};
use support::payload;

/// The files copied: each larger than a mark's least size, so each is marked while it runs.
const FILES: [(&str, usize); 3] = [
    ("a.bin", 2 * 1024 * 1024 + 17),
    ("b.bin", 2 * 1024 * 1024 + 4099),
    ("c.bin", 3 * 1024 * 1024 + 1),
];

/// What a database keeps: the jobs by record, with the file each was writing.
#[derive(Default)]
struct Disk {
    rows: Mutex<BTreeMap<i64, (JobSpec, Option<FileMark>)>>,
    next: Mutex<i64>,
}

impl Disk {
    fn rows(&self) -> MutexGuard<'_, BTreeMap<i64, (JobSpec, Option<FileMark>)>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The jobs as the next start reads them.
    fn saved(&self) -> Vec<SavedJob> {
        self.rows()
            .iter()
            .map(|(record, (spec, mark))| SavedJob {
                record: *record,
                spec: spec.clone(),
                mark: mark.clone(),
            })
            .collect()
    }
}

#[async_trait]
impl TransferStore for Disk {
    async fn save(&self, spec: &JobSpec) -> Option<i64> {
        let record = {
            let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
            *next += 1;
            *next
        };
        self.rows().insert(record, (spec.clone(), None));
        Some(record)
    }
    async fn mark(&self, record: i64, mark: &FileMark) {
        if let Some(row) = self.rows().get_mut(&record) {
            row.1 = Some(mark.clone());
        }
    }
    async fn forget(&self, record: i64) {
        self.rows().remove(&record);
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime")
}

fn queue(scheduling: Scheduling, local: Backend, store: &Arc<Disk>) -> Queue {
    let sink: EventSink = Arc::new(|_: rc_ops::JobEvent| {});
    let queue = Queue::new(
        QueueConfig {
            scheduling,
            ..QueueConfig::default()
        },
        sink,
    );
    queue.register(BackendId::Local, local);
    queue.set_store(Arc::clone(store) as Arc<dyn TransferStore>);
    queue
}

fn part_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".rcpart");
    path.with_file_name(name)
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |meta| meta.len())
}

/// Waits on the real clock until `done` holds, at most ten seconds.
async fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "{what}: not in time");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Three copies from disk to disk, killed while `running` of them write their part files, then
/// resumed after the restart.
fn kill_and_restart(scheduling: Scheduling, running: usize) {
    let root = tempfile::tempdir().expect("temp dir");
    let src = root.path().join("src");
    let out = root.path().join("out");
    std::fs::create_dir(&src).expect("mkdir");
    let mut sources = Vec::new();
    for (seed, (name, len)) in FILES.iter().enumerate() {
        let data = payload(*len, u8::try_from(seed).expect("three") + 3);
        std::fs::write(src.join(name), &data).expect("write");
        sources.push((out.join(name), data));
    }
    let store = Arc::new(Disk::default());

    let first = runtime();
    let killed = first.block_on(async {
        let queue = queue(
            scheduling,
            Backend::new(Arc::new(Trickle::default())),
            &store,
        );
        for (name, _) in FILES {
            let spec = JobSpec {
                kind: JobKind::Copy {
                    sources: vec![VfsPath::local(src.join(name))],
                    target: VfsPath::local(&out),
                },
                options: JobOptions::default(),
            };
            queue.submit(spec).expect("accepted");
        }
        // Every job is kept from its submission; `running` of them are well into their file.
        wait_until("the copies are in their middle", || {
            let parts = sources
                .iter()
                .filter(|(target, _)| size(&part_of(target)) >= 256 * 1024)
                .count();
            store.rows().len() == FILES.len() && parts >= running
        })
        .await;
        queue
    });
    // The kill: the queue never hears of it, and nothing after this line runs in its runtime.
    std::mem::forget(killed);
    first.shutdown_background();

    for (target, data) in &sources {
        assert!(
            !target.exists() || std::fs::read(target).expect("read") == *data,
            "{} is incomplete under its own name",
            target.display()
        );
    }
    let saved = store.saved();
    assert_eq!(saved.len(), FILES.len(), "every job is in the store");
    let marked = saved.iter().filter(|job| job.mark.is_some()).count();
    assert!(
        marked >= running,
        "{marked} marks for {running} running copies"
    );
    assert!(
        saved
            .iter()
            .filter_map(|job| job.mark.as_ref())
            .all(|mark| mark.size >= MARK_MIN_BYTES)
    );

    let second = runtime();
    second.block_on(async {
        let queue = queue(
            Scheduling::default(),
            Backend::new(Arc::new(LocalFs::new())),
            &store,
        );
        let mut ids = Vec::new();
        for job in saved {
            let restored = queue.restore(job).expect("restored");
            assert_eq!(restored.state, JobState::Paused);
            assert!(restored.resumable);
            ids.push(restored.id);
        }
        for id in &ids {
            queue.resume(*id).expect("resume");
        }
        for id in ids {
            let done = tokio::time::timeout(Duration::from_secs(30), queue.wait(id))
                .await
                .expect("in time")
                .expect("known");
            assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
        }
    });
    for (target, data) in &sources {
        assert_eq!(
            std::fs::read(target).expect("read"),
            *data,
            "{}",
            target.display()
        );
        assert!(!part_of(target).exists(), "no part is left");
    }
    assert!(store.rows().is_empty(), "the store forgot every job");
}

/// Three copies running at once, each in its middle when the process dies.
#[test]
fn three_copies_killed_in_their_middle_resume_byte_for_byte() {
    kill_and_restart(Scheduling::Parallel, FILES.len());
}

/// One copy at a time on one disk: two of the three still wait for their turn when the process
/// dies, and come back as well.
#[test]
fn waiting_copies_are_back_after_the_restart_too() {
    kill_and_restart(Scheduling::PerTargetBackend, 1);
}
