//! Overwriting on a server whose rename does not replace (SFTP version 3, RC-110-25): the old
//! file goes only once the new one is whole under its part name, and when the rename after the
//! removal fails the part stays - it is the only copy on the server. A server with a rename
//! that replaces never has its target removed.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::support::{Harness, exists, mem, payload, read, write};
use async_trait::async_trait;
use rc_ops::{Backend, ConflictChoice, ErrorPolicy, JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsError, VfsPath};

const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };

/// A server whose plain rename never replaces. `full` refuses a rename onto a free name, as a
/// quota reached after the old file went would; `posix` gives it the rename that replaces.
struct Version3 {
    fs: MemoryFs,
    full: Arc<AtomicBool>,
    posix: bool,
    removed: Arc<Mutex<Vec<PathBuf>>>,
}

#[async_trait]
impl rc_vfs::testing::Forward for Version3 {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.fs.stat(to).await.is_ok() {
            return Err(VfsError::AlreadyExists {
                path: to.to_path_buf(),
            });
        }
        if self.full.load(Ordering::SeqCst) {
            return Err(VfsError::PermissionDenied {
                path: to.to_path_buf(),
            });
        }
        self.fs.rename(from, to).await
    }
    async fn rename_replacing(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.posix {
            return self.fs.rename(from, to).await;
        }
        rc_vfs::testing::Forward::rename(self, from, to).await
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        self.removed
            .lock()
            .expect("removed")
            .push(path.to_path_buf());
        self.fs.remove_file(path).await
    }
}

/// What a test holds of its server: the files, the switch, what was removed.
struct Served {
    fs: MemoryFs,
    full: Arc<AtomicBool>,
    removed: Arc<Mutex<Vec<PathBuf>>>,
}

/// The server registered with `/in/big.bin` on it, and a new `/big.bin` on `Memory(1)`.
async fn start(harness: &Harness, posix: bool) -> (Served, Vec<u8>) {
    let served = Served {
        fs: MemoryFs::new(),
        full: Arc::default(),
        removed: Arc::default(),
    };
    let server = Version3 {
        fs: served.fs.clone(),
        full: Arc::clone(&served.full),
        posix,
        removed: Arc::clone(&served.removed),
    };
    harness
        .queue
        .register(SERVER, Backend::new(Arc::new(server)));
    write(&served.fs, "/in/big.bin", b"the old file").await;
    let data = payload(20_000, 9);
    write(&harness.memory(1), "/big.bin", &data).await;
    (served, data)
}

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: PathBuf::from(path),
    }
}

fn overwriting(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions {
            on_conflict: Some(ConflictChoice::Overwrite),
            on_error: ErrorPolicy::Cancel,
            ..JobOptions::default()
        },
    }
}

fn upload() -> JobSpec {
    overwriting(JobKind::Copy {
        sources: vec![mem(1, "/big.bin")],
        target: on_server("/in"),
    })
}

#[tokio::test]
async fn an_overwrite_removes_the_old_file_and_renames_the_part() {
    let harness = Harness::start();
    let (served, data) = start(&harness, false).await;
    let done = harness.run(upload()).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&served.fs, "/in/big.bin").await, data);
    assert!(!exists(&served.fs, "/in/big.bin.rcpart").await);
    assert_eq!(
        *served.removed.lock().expect("removed"),
        [PathBuf::from("/in/big.bin")]
    );
}

/// The failure of the second rename: before RC-110-25 the part was removed after the old file,
/// and the server had neither.
#[tokio::test]
async fn a_rename_refused_after_the_old_file_went_keeps_the_part() {
    let harness = Harness::start();
    let (served, data) = start(&harness, false).await;
    served.full.store(true, Ordering::SeqCst);
    let done = harness.run(upload()).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    let failure = &done.failures[0];
    assert_eq!(failure.code, "permission_denied");
    assert_eq!(
        failure.target,
        Some(Box::new(on_server("/in/big.bin"))),
        "the side that failed"
    );
    assert!(
        !exists(&served.fs, "/in/big.bin").await,
        "the old file went"
    );
    assert_eq!(
        read(&served.fs, "/in/big.bin.rcpart").await,
        data,
        "the new file stays under its part name"
    );
    assert_eq!(
        *served.removed.lock().expect("removed"),
        [PathBuf::from("/in/big.bin")],
        "the part was not removed"
    );
}

#[tokio::test]
async fn a_server_with_a_rename_that_replaces_never_loses_its_target() {
    let harness = Harness::start();
    let (served, data) = start(&harness, true).await;
    // The plain rename would refuse: the one that replaces is the only one used.
    served.full.store(true, Ordering::SeqCst);
    let done = harness.run(upload()).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(read(&served.fs, "/in/big.bin").await, data);
    assert!(!exists(&served.fs, "/in/big.bin.rcpart").await);
    assert!(
        served.removed.lock().expect("removed").is_empty(),
        "nothing was removed"
    );
}

/// A move within the server is the same rename: when it fails after the old target went, the
/// source is still where it was.
#[tokio::test]
async fn a_move_refused_after_the_old_file_went_keeps_the_source() {
    let harness = Harness::start();
    let (served, _data) = start(&harness, false).await;
    write(&served.fs, "/out/big.bin", b"the new file").await;
    served.full.store(true, Ordering::SeqCst);
    let done = harness
        .run(overwriting(JobKind::Move {
            sources: vec![on_server("/out/big.bin")],
            target: on_server("/in"),
        }))
        .await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(done.failures[0].code, "permission_denied");
    assert_eq!(read(&served.fs, "/out/big.bin").await, b"the new file");
    assert!(!exists(&served.fs, "/in/big.bin").await);
}
