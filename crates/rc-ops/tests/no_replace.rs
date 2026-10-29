//! A move renamed within one disk never replaces what took the free name after the conflict
//! check (RC-080-52, RC-080-56): a file is asked about again, a folder is merged into, and an
//! existing target moved aside never lands on a name taken meanwhile.

mod support;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use rc_ops::{Backend, ConflictChoice, ErrorPolicy, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, Entry, MemoryFs, Vfs, VfsError, VfsPath};
use support::{Harness, exists, read, write};

/// A disk on which `plant` takes `trap`'s name right after the first look at it: the race
/// between the conflict check and a move's rename. Its rename that never replaces is the
/// trait's default, which looks again.
struct LateArrival {
    inner: MemoryFs,
    trap: PathBuf,
    plant: &'static str,
    sprung: AtomicBool,
}

#[async_trait]
impl rc_vfs::testing::Forward for LateArrival {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        if path == self.trap && !self.sprung.swap(true, Ordering::SeqCst) {
            write(&self.inner, self.plant, b"theirs").await;
            return Err(VfsError::NotFound {
                path: path.to_path_buf(),
            });
        }
        self.inner.stat(path).await
    }
}

fn local(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Local,
        path: PathBuf::from(path),
    }
}

/// `MemoryFs` registered as the local disk, with the trap set at `trap`.
fn disk(harness: &Harness, trap: &str, plant: &'static str) -> MemoryFs {
    let disk = MemoryFs::new();
    harness.queue.register(
        BackendId::Local,
        Backend::new(Arc::new(LateArrival {
            inner: disk.clone(),
            trap: PathBuf::from(trap),
            plant,
            sprung: AtomicBool::new(false),
        })),
    );
    disk
}

fn moving(source: &str, on_conflict: Option<ConflictChoice>) -> JobSpec {
    JobSpec {
        kind: JobKind::Move {
            sources: vec![local(source)],
            target: local("/in"),
        },
        options: JobOptions {
            on_error: ErrorPolicy::Skip,
            on_conflict,
            ..JobOptions::default()
        },
    }
}

/// The file that arrived is a conflict like one found first: here answered "rename", so the
/// moved file goes beside it and nothing is replaced.
#[tokio::test]
async fn a_local_move_does_not_replace_a_file_that_arrived_meanwhile() {
    let harness = Harness::start();
    let disk = disk(&harness, "/in/a.bin", "/in/a.bin");
    write(&disk, "/a.bin", b"mine").await;
    disk.mkdir(Path::new("/in"), false).await.expect("mkdir");
    let done = harness
        .run(moving("/a.bin", Some(ConflictChoice::Rename)))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{done:?}");
    assert_eq!(read(&disk, "/in/a.bin").await, b"theirs", "not replaced");
    assert_eq!(read(&disk, "/in/a (2).bin").await, b"mine", "asked again");
    assert!(!exists(&disk, "/a.bin").await, "moved");
}

/// A folder that took the free name is merged into: what it holds stays, the moved folder's
/// files go in beside it.
#[tokio::test]
async fn a_local_move_merges_into_a_folder_that_arrived_meanwhile() {
    let harness = Harness::start();
    let disk = disk(&harness, "/in/dir", "/in/dir/theirs.txt");
    write(&disk, "/dir/mine.txt", b"mine").await;
    disk.mkdir(Path::new("/in"), false).await.expect("mkdir");
    let done = harness.run(moving("/dir", None)).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{done:?}");
    assert_eq!(read(&disk, "/in/dir/theirs.txt").await, b"theirs", "kept");
    assert_eq!(read(&disk, "/in/dir/mine.txt").await, b"mine", "merged");
    assert!(!exists(&disk, "/dir").await, "the emptied source goes");
}

/// "Rename the existing target": the numbered name it found is taken before its rename, so the
/// next free one is used - the existing target is not lost, the late file not replaced.
#[tokio::test]
async fn an_existing_target_moved_aside_skips_a_name_taken_meanwhile() {
    let harness = Harness::start();
    let disk = disk(&harness, "/in/a (2).bin", "/in/a (2).bin");
    write(&disk, "/a.bin", b"mine").await;
    write(&disk, "/in/a.bin", b"old").await;
    let done = harness
        .run(moving("/a.bin", Some(ConflictChoice::RenameExisting)))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{done:?}");
    assert_eq!(read(&disk, "/in/a.bin").await, b"mine");
    assert_eq!(
        read(&disk, "/in/a (2).bin").await,
        b"theirs",
        "not replaced"
    );
    assert_eq!(read(&disk, "/in/a (3).bin").await, b"old", "moved aside");
    assert!(!exists(&disk, "/a.bin").await, "moved");
}
