//! A multi-rename that fails half way (RC-080-64): with roll-back on, the job reverses its own
//! steps when entries are under temporary names, and its report says so; off, or when the
//! reversal fails too, the report lists each parked entry. Each step renames without replacing,
//! so a name taken in the moment before the rename fails the step and is kept.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use crate::support::{Harness, exists, mem, read, write};
use async_trait::async_trait;
use rc_ops::{
    Backend, ErrorPolicy, JobKind, JobOptions, JobReport, JobSnapshot, JobSpec, JobState,
    ParkedName,
};
use rc_rename::RenameStep;
use rc_vfs::{
    BackendId, Capabilities, Entry, LocalFs, MemoryFs, ReadStream, SetAttributes, Vfs, VfsError,
    VfsPath, WriteMode, WriteStream,
};

/// A disk that refuses to rename the names in `refused` (an open file on Windows).
struct Refusing {
    inner: MemoryFs,
    refused: Vec<&'static str>,
}

#[async_trait]
impl rc_vfs::testing::Forward for Refusing {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        let name = from
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if self.refused.contains(&name) {
            return Err(VfsError::PermissionDenied {
                path: from.to_path_buf(),
            });
        }
        self.inner.rename(from, to).await
    }
}

fn step(from: &str, to: &str) -> RenameStep {
    RenameStep {
        from: from.to_owned(),
        to: to.to_owned(),
    }
}

/// `a.txt` and `b.txt` swapped, as `rc_rename::schedule` orders it.
fn swap() -> Vec<RenameStep> {
    vec![
        step("a.txt", ".rc-rename-t-0"),
        step("b.txt", "a.txt"),
        step(".rc-rename-t-0", "b.txt"),
    ]
}

/// `/d` with `a.txt` and `b.txt`, each holding its own name, on a disk refusing `refused`.
async fn disk(harness: &Harness, refused: Vec<&'static str>) -> MemoryFs {
    let inner = MemoryFs::new();
    for name in ["a.txt", "b.txt"] {
        write(&inner, &format!("/d/{name}"), name.as_bytes()).await;
    }
    harness.queue.register(
        BackendId::Memory(1),
        Backend::new(Arc::new(Refusing {
            inner: inner.clone(),
            refused,
        })),
    );
    inner
}

async fn run(harness: &Harness, rollback: bool) -> JobSnapshot {
    harness
        .run(JobSpec {
            kind: JobKind::rename(mem(1, "/d"), swap()).rolling_back(rollback),
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        })
        .await
}

async fn content(fs: &MemoryFs, name: &str) -> String {
    String::from_utf8(read(fs, &format!("/d/{name}")).await).expect("utf-8")
}

fn parked_a() -> Vec<ParkedName> {
    vec![ParkedName {
        temp: ".rc-rename-t-0".to_owned(),
        old: "a.txt".to_owned(),
        new: "b.txt".to_owned(),
    }]
}

#[tokio::test]
async fn a_swap_failing_in_its_middle_is_rolled_back() {
    let harness = Harness::start();
    let fs = disk(&harness, vec!["b.txt"]).await;
    let done = run(&harness, true).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(
        done.failures[0].code, "permission_denied",
        "the first error"
    );
    assert_eq!(
        done.report,
        Some(JobReport::RenameStopped {
            rolled_back: true,
            parked: Vec::new(),
        })
    );
    assert_eq!(content(&fs, "a.txt").await, "a.txt");
    assert_eq!(content(&fs, "b.txt").await, "b.txt");
    assert!(!exists(&fs, "/d/.rc-rename-t-0").await, "nothing parked");
}

#[tokio::test]
async fn without_roll_back_the_report_lists_the_parked_entry() {
    let harness = Harness::start();
    let fs = disk(&harness, vec!["b.txt"]).await;
    let done = run(&harness, false).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(
        done.report,
        Some(JobReport::RenameStopped {
            rolled_back: false,
            parked: parked_a(),
        })
    );
    assert_eq!(content(&fs, ".rc-rename-t-0").await, "a.txt");
}

#[tokio::test]
async fn a_roll_back_that_fails_too_lists_every_parked_entry() {
    let harness = Harness::start();
    let fs = disk(&harness, vec!["b.txt", ".rc-rename-t-0"]).await;
    let done = run(&harness, true).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(
        done.failures.len(),
        2,
        "the step and its reversal: {done:?}"
    );
    assert_eq!(
        done.report,
        Some(JobReport::RenameStopped {
            rolled_back: false,
            parked: parked_a(),
        })
    );
    assert_eq!(content(&fs, ".rc-rename-t-0").await, "a.txt");
}

/// The local disk, on which `plant` takes `trap`'s name in the moment before the first rename
/// onto it - and also at a look at it, the race the step's look and rename had (RC-080-64).
struct Racing {
    inner: LocalFs,
    trap: PathBuf,
    sprung: AtomicBool,
}

impl Racing {
    fn spring(&self, path: &Path) {
        if path == self.trap && !self.sprung.swap(true, Ordering::SeqCst) {
            std::fs::write(path, b"theirs").expect("plant");
        }
    }
}

#[async_trait]
impl Vfs for Racing {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    async fn list(&self, dir: &Path) -> rc_vfs::Result<Vec<Entry>> {
        self.inner.list(dir).await
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        let looked = self.inner.stat(path).await;
        self.spring(path);
        looked
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        self.inner.open_read(path).await
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        self.inner.open_write(path, mode).await
    }
    async fn mkdir(&self, path: &Path, recursive: bool) -> rc_vfs::Result<()> {
        self.inner.mkdir(path, recursive).await
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        self.inner.rename(from, to).await
    }
    async fn rename_no_replace(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        self.spring(to);
        self.inner.rename_no_replace(from, to).await
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        self.inner.remove_file(path).await
    }
    async fn remove_dir(&self, path: &Path, recursive: bool) -> rc_vfs::Result<()> {
        self.inner.remove_dir(path, recursive).await
    }
    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> rc_vfs::Result<()> {
        self.inner.set_mtime(path, modified).await
    }
    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> rc_vfs::Result<()> {
        self.inner.set_attributes(path, change).await
    }
}

#[tokio::test]
async fn a_name_taken_in_the_moment_before_the_rename_is_kept() {
    let harness = Harness::start();
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("a.txt"), b"mine").expect("write");
    harness.queue.register(
        BackendId::Local,
        Backend::new(Arc::new(Racing {
            inner: LocalFs::new(),
            trap: dir.path().join("b.txt"),
            sprung: AtomicBool::new(false),
        })),
    );
    let done = harness
        .run(JobSpec {
            kind: JobKind::rename(
                VfsPath {
                    backend: BackendId::Local,
                    path: dir.path().to_path_buf(),
                },
                vec![step("a.txt", "b.txt")],
            ),
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "already_exists");
    assert_eq!(
        std::fs::read(dir.path().join("b.txt")).expect("read"),
        b"theirs"
    );
    assert_eq!(
        std::fs::read(dir.path().join("a.txt")).expect("read"),
        b"mine"
    );
}
