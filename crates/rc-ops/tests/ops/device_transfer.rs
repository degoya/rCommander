//! Copy and move onto and off a portable device (RC-100-61): the queue treats
//! `BackendId::Device` like any backend that is no disk and no server - the file is written in
//! place, never as a part file (MTP renames nothing reliably and resumes nothing), and a move off
//! the device removes the source once the copy is complete. An in-memory backend stands in for
//! the device; the device backend's own streaming is tested in `rc-vfs` (`device/fs_tests.rs`,
//! `device/upload_tests.rs`). A file copied onto a device is opened with its size, so the device
//! streams it as it is written (RC-110-16); a copy that fails or is cancelled lets the device's
//! stream go without shutting it down, and - overwriting - leaves the old file, which the device
//! keeps until the new one is whole (RC-110-25).

use std::{
    fs, io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use crate::support::{Harness, exists, payload, read, write};
use async_trait::async_trait;
use rc_ops::{
    Backend, ConflictChoice, ErrorPolicy, JobChange, JobKind, JobOptions, JobSpec, JobState,
};
use rc_vfs::{BackendId, MemoryFs, Vfs, VfsPath, WriteMode, WriteStream};

/// A target that records the size each file was opened with (`None`: without one).
struct Sizes {
    fs: MemoryFs,
    opened: Arc<Mutex<Vec<Option<u64>>>>,
}

#[async_trait]
impl rc_vfs::testing::Forward for Sizes {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        self.opened.lock().expect("opened").push(None);
        self.fs.open_write(path, mode).await
    }
    async fn open_write_sized(
        &self,
        path: &Path,
        mode: WriteMode,
        size: u64,
    ) -> rc_vfs::Result<WriteStream> {
        self.opened.lock().expect("opened").push(Some(size));
        self.fs.open_write(path, mode).await
    }
}

/// [`Sizes`] registered as `backend`; what it recorded.
fn sizes(harness: &Harness, backend: BackendId) -> Arc<Mutex<Vec<Option<u64>>>> {
    let opened = Arc::new(Mutex::new(Vec::new()));
    let target = Sizes {
        fs: MemoryFs::new(),
        opened: Arc::clone(&opened),
    };
    harness
        .queue
        .register(backend, Backend::new(Arc::new(target)));
    opened
}

fn device(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Device {
            device: "usb#phone".to_owned(),
        },
        path: PathBuf::from(path),
    }
}

fn spec(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions::default(),
    }
}

#[tokio::test]
async fn a_file_is_copied_onto_the_device_in_place() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("photo.jpg");
    fs::write(&source, payload(300_000, 7)).expect("write");
    let harness = Harness::start();
    harness.local();
    let phone = harness.memory_as(BackendId::Device {
        device: "usb#phone".to_owned(),
    });
    write(&phone, "/Internal storage/keep.txt", b"k").await;

    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![VfsPath::local(&source)],
            target: device("/Internal storage"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.progress.bytes_done, 300_000);
    assert_eq!(
        read(&phone, "/Internal storage/photo.jpg").await,
        payload(300_000, 7)
    );
    assert!(
        !exists(&phone, "/Internal storage/photo.jpg.rcpart").await,
        "no part file on a device"
    );
    assert!(source.exists(), "the source stays");
}

#[tokio::test]
async fn a_move_off_the_device_removes_the_source_after_the_copy() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let phone = harness.memory_as(BackendId::Device {
        device: "usb#phone".to_owned(),
    });
    write(&phone, "/Internal storage/DCIM/a.jpg", b"jpeg").await;

    let done = harness
        .run(spec(JobKind::Move {
            sources: vec![device("/Internal storage/DCIM/a.jpg")],
            target: VfsPath::local(root.path()),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(fs::read(root.path().join("a.jpg")).expect("read"), b"jpeg");
    assert!(
        !exists(&phone, "/Internal storage/DCIM/a.jpg").await,
        "the source is gone"
    );
}

#[tokio::test]
async fn a_file_is_opened_on_the_device_with_its_size() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("video.mp4");
    fs::write(&source, payload(300_000, 3)).expect("write");
    let harness = Harness::start();
    harness.local();
    let opened = sizes(
        &harness,
        BackendId::Device {
            device: "usb#phone".to_owned(),
        },
    );

    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![VfsPath::local(&source)],
            target: device("/Internal storage"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.progress.bytes_done, 300_000);
    assert_eq!(*opened.lock().expect("opened"), [Some(300_000)]);
}

#[tokio::test]
async fn a_file_onto_another_target_is_opened_without_a_size() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("video.mp4");
    fs::write(&source, payload(1000, 3)).expect("write");
    let harness = Harness::start();
    harness.local();
    let opened = sizes(&harness, BackendId::Memory(2));

    let done = harness
        .run(spec(JobKind::Copy {
            sources: vec![VfsPath::local(&source)],
            target: VfsPath {
                backend: BackendId::Memory(2),
                path: PathBuf::from("/"),
            },
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(*opened.lock().expect("opened"), [None]);
}

/// A stream whose every write fails, and which notes a shutdown.
struct Broken {
    shut_down: Arc<AtomicBool>,
}

impl tokio::io::AsyncWrite for Broken {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::other("the cable was pulled")))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.shut_down.store(true, Ordering::SeqCst);
        Poll::Ready(Ok(()))
    }
}

/// A target whose files are [`Broken`] streams.
struct BrokenTarget {
    fs: MemoryFs,
    shut_down: Arc<AtomicBool>,
}

#[async_trait]
impl rc_vfs::testing::Forward for BrokenTarget {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn open_write(&self, _path: &Path, _mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        Ok(Box::new(Broken {
            shut_down: Arc::clone(&self.shut_down),
        }))
    }
    async fn open_write_sized(
        &self,
        path: &Path,
        mode: WriteMode,
        _size: u64,
    ) -> rc_vfs::Result<WriteStream> {
        rc_vfs::testing::Forward::open_write(self, path, mode).await
    }
}

/// Copies a file onto a [`BrokenTarget`] registered as `backend`; whether the failed copy shut
/// its stream down.
async fn shut_down_after_a_failure(backend: BackendId) -> bool {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("video.mp4");
    fs::write(&source, payload(300_000, 5)).expect("write");
    let harness = Harness::start();
    harness.local();
    let shut_down = Arc::new(AtomicBool::new(false));
    let target = BrokenTarget {
        fs: MemoryFs::new(),
        shut_down: Arc::clone(&shut_down),
    };
    harness
        .queue
        .register(backend.clone(), Backend::new(Arc::new(target)));
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(&source)],
                target: VfsPath {
                    backend,
                    path: PathBuf::from("/"),
                },
            },
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    shut_down.load(Ordering::SeqCst)
}

/// A copy onto a device that fails (a cancel ends the same way) lets the stream go: shut down,
/// a stream that kept the file on this machine would still upload what it has (RC-110-16).
/// Any other target's stream is shut down after a failure as before.
#[tokio::test]
async fn a_failed_copy_onto_the_device_does_not_shut_its_stream_down() {
    let phone = BackendId::Device {
        device: "usb#phone".to_owned(),
    };
    assert!(!shut_down_after_a_failure(phone).await, "the device");
    assert!(
        shut_down_after_a_failure(BackendId::Memory(2)).await,
        "another target"
    );
}

/// A device as MTP is one: a file is on it only once its stream is shut down, the file it
/// replaces stays until then, and a stream let go leaves nothing. Its streams fail every write
/// (`broken`) or take every byte and keep none.
struct Phone {
    fs: MemoryFs,
    broken: bool,
}

#[async_trait]
impl rc_vfs::testing::Forward for Phone {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn open_write(&self, _path: &Path, _mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        if self.broken {
            return Ok(Box::new(Broken {
                shut_down: Arc::default(),
            }));
        }
        Ok(Box::new(tokio::io::sink()))
    }
    async fn open_write_sized(
        &self,
        path: &Path,
        mode: WriteMode,
        _size: u64,
    ) -> rc_vfs::Result<WriteStream> {
        rc_vfs::testing::Forward::open_write(self, path, mode).await
    }
}

/// A [`Phone`] with an old `/video.mp4`, and a job that overwrites it with a new one.
async fn overwriting_on_a_phone(
    harness: &Harness,
    root: &Path,
    broken: bool,
) -> (MemoryFs, JobSpec) {
    let source = root.join("video.mp4");
    fs::write(&source, payload(300_000, 5)).expect("write");
    harness.local();
    let phone = MemoryFs::new();
    write(&phone, "/video.mp4", b"the old video").await;
    harness.queue.register(
        BackendId::Device {
            device: "usb#phone".to_owned(),
        },
        Backend::new(Arc::new(Phone {
            fs: phone.clone(),
            broken,
        })),
    );
    let job = JobSpec {
        kind: JobKind::Copy {
            sources: vec![VfsPath::local(&source)],
            target: device("/"),
        },
        options: JobOptions {
            on_conflict: Some(ConflictChoice::Overwrite),
            on_error: ErrorPolicy::Cancel,
            ..JobOptions::default()
        },
    };
    (phone, job)
}

/// RC-110-25: the failed copy removed what was under the target's name - on a device the old
/// file, which the upload had not replaced yet.
#[tokio::test]
async fn a_failed_overwrite_on_the_device_leaves_the_old_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    let (phone, job) = overwriting_on_a_phone(&harness, root.path(), true).await;
    let done = harness.run(job).await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(read(&phone, "/video.mp4").await, b"the old video");
}

#[tokio::test]
async fn a_cancelled_overwrite_on_the_device_leaves_the_old_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    let (phone, job) = overwriting_on_a_phone(&harness, root.path(), false).await;
    let cancelled = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&cancelled);
    harness.on_event(move |queue, event| {
        let inside = matches!(
            &event.change,
            JobChange::Progress { progress }
                if progress.bytes_done > 0 && progress.bytes_done < 300_000
        );
        if inside && !seen.swap(true, Ordering::SeqCst) {
            queue.cancel(event.job).expect("cancel");
        }
    });
    let done = harness.run(job).await;
    assert!(
        cancelled.load(Ordering::SeqCst),
        "cancelled inside the file"
    );
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert_eq!(read(&phone, "/video.mp4").await, b"the old video");
}

/// A new file on the device that fails is still cleared away: nothing is kept under a name
/// that was free.
#[tokio::test]
async fn a_failed_new_file_on_the_device_is_removed_as_before() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("video.mp4");
    fs::write(&source, payload(300_000, 5)).expect("write");
    let harness = Harness::start();
    harness.local();
    let phone = MemoryFs::new();
    let removed = Arc::new(Mutex::new(Vec::new()));
    harness.queue.register(
        BackendId::Device {
            device: "usb#phone".to_owned(),
        },
        Backend::new(Arc::new(Removals {
            phone: Phone {
                fs: phone.clone(),
                broken: true,
            },
            removed: Arc::clone(&removed),
        })),
    );
    let done = harness
        .run(JobSpec {
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(&source)],
                target: device("/"),
            },
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        })
        .await;
    assert_eq!(done.state, JobState::Failed, "{done:?}");
    assert_eq!(
        *removed.lock().expect("removed"),
        [PathBuf::from("/video.mp4")]
    );
}

/// A [`Phone`] that notes which files were removed from it.
struct Removals {
    phone: Phone,
    removed: Arc<Mutex<Vec<PathBuf>>>,
}

#[async_trait]
impl rc_vfs::testing::Forward for Removals {
    fn inner(&self) -> &dyn Vfs {
        &self.phone
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        self.removed
            .lock()
            .expect("removed")
            .push(path.to_path_buf());
        Vfs::remove_file(&self.phone, path).await
    }
}
