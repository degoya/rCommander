//! RC-080-37: a move makes its copy durable before the source goes - `Vfs::sync` on the target
//! once the file landed - and keeps the source when that fails.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::support::{Harness, exists, mem, read, write};
use async_trait::async_trait;
use rc_ops::{Backend, ErrorPolicy, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsError};

/// A target that records each `sync`, and refuses it when `fail` is set.
struct Synced {
    fs: MemoryFs,
    synced: Arc<Mutex<Vec<PathBuf>>>,
    fail: bool,
}

#[async_trait]
impl rc_vfs::testing::Forward for Synced {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn sync(&self, path: &Path) -> rc_vfs::Result<()> {
        self.synced.lock().expect("synced").push(path.to_path_buf());
        if self.fail {
            return Err(VfsError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::other("the drive was pulled"),
            });
        }
        Ok(())
    }
}

fn target(harness: &Harness, fail: bool) -> (MemoryFs, Arc<Mutex<Vec<PathBuf>>>) {
    let fs = MemoryFs::new();
    let synced = Arc::new(Mutex::new(Vec::new()));
    let backend = Synced {
        fs: fs.clone(),
        synced: Arc::clone(&synced),
        fail,
    };
    harness
        .queue
        .register(BackendId::Memory(2), Backend::new(Arc::new(backend)));
    (fs, synced)
}

fn moving(on_error: ErrorPolicy) -> JobSpec {
    JobSpec {
        kind: JobKind::Move {
            sources: vec![mem(1, "/a.txt")],
            target: mem(2, "/"),
        },
        options: JobOptions {
            on_error,
            ..JobOptions::default()
        },
    }
}

#[tokio::test]
async fn a_move_syncs_its_copy_before_the_source_goes() {
    let harness = Harness::start();
    let source = harness.memory(1);
    write(&source, "/a.txt", b"alpha").await;
    let (dest, synced) = target(&harness, false);
    let done = harness.run(moving(ErrorPolicy::default())).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&dest, "/a.txt").await, b"alpha");
    assert!(!exists(&source, "/a.txt").await, "moved");
    assert_eq!(*synced.lock().expect("synced"), [PathBuf::from("/a.txt")]);
}

#[tokio::test]
async fn a_copy_that_cannot_be_synced_keeps_its_source() {
    let harness = Harness::start();
    let source = harness.memory(1);
    write(&source, "/a.txt", b"alpha").await;
    let (_dest, synced) = target(&harness, true);
    let done = harness.run(moving(ErrorPolicy::Skip)).await;
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert!(exists(&source, "/a.txt").await, "the source stays");
    assert!(!synced.lock().expect("synced").is_empty());
}
