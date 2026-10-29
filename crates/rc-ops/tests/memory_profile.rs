//! A 4 GiB file goes to a server with constant memory (RC-060-05), and from one server to another
//! (RC-060-06): a source that makes its bytes as they are read and a server that counts and drops
//! them, so nothing but the copy itself can hold memory. The peak resident size of the process
//! (`VmHWM`, Linux) may grow by at most [`LIMIT`] over the whole copy - a sixty-fourth of the
//! file. Alone in its test binary, and the two copies one after the other ([`ONE_AT_A_TIME`]), so
//! no other test raises the peak.

use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use rc_ops::{Backend, EventSink, JobKind, JobOptions, JobSpec, JobState, Queue, QueueConfig};
use rc_vfs::{
    Attributes, BackendId, Capabilities, Entry, EntryKind, ReadStream, SetAttributes, Vfs,
    VfsError, VfsPath, WriteMode, WriteStream,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const FILE: u64 = 4 << 30;
/// How far the peak resident size may grow while the file is copied.
const LIMIT: u64 = 64 << 20;

fn entry(name: &str, kind: EntryKind, size: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size,
        modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000)),
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// What a backend that has only `only` answers for `path`.
fn only(path: &Path, only: &str, found: Entry) -> rc_vfs::Result<Entry> {
    if path == Path::new(only) {
        Ok(found)
    } else {
        Err(VfsError::NotFound {
            path: path.to_path_buf(),
        })
    }
}

/// Nothing but files to read or write: no links, no modes, no times.
fn capabilities() -> Capabilities {
    Capabilities {
        rename_across_dirs: true,
        symlinks: false,
        case_sensitive: true,
        unix_mode: false,
        set_mtime: false,
        free_space: false,
    }
}

fn refused(path: &Path) -> VfsError {
    VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "this test backend",
    }
}

/// `/big.bin` of [`FILE`] bytes, made as they are read.
struct Endless;

struct Bytes {
    left: u64,
}

impl AsyncRead for Bytes {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let count = usize::try_from(self.left)
            .unwrap_or(usize::MAX)
            .min(buf.remaining());
        buf.initialize_unfilled_to(count).fill(0x5A);
        buf.advance(count);
        self.left -= count as u64;
        Poll::Ready(Ok(()))
    }
}

/// A server that keeps nothing: it counts what is written and takes every change.
struct Drain {
    written: Arc<AtomicU64>,
}

struct Counter(Arc<AtomicU64>);

impl AsyncWrite for Counter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[async_trait]
impl Vfs for Endless {
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    async fn list(&self, _dir: &Path) -> rc_vfs::Result<Vec<Entry>> {
        Ok(vec![entry("big.bin", EntryKind::File, FILE)])
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        only(path, "/big.bin", entry("big.bin", EntryKind::File, FILE))
    }
    async fn open_read(&self, _path: &Path) -> rc_vfs::Result<ReadStream> {
        Ok(Box::new(Bytes { left: FILE }))
    }
    async fn open_write(&self, path: &Path, _mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        Err(refused(path))
    }
    async fn mkdir(&self, path: &Path, _recursive: bool) -> rc_vfs::Result<()> {
        Err(refused(path))
    }
    async fn rename(&self, from: &Path, _to: &Path) -> rc_vfs::Result<()> {
        Err(refused(from))
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        Err(refused(path))
    }
    async fn remove_dir(&self, path: &Path, _recursive: bool) -> rc_vfs::Result<()> {
        Err(refused(path))
    }
    async fn set_mtime(&self, path: &Path, _modified: SystemTime) -> rc_vfs::Result<()> {
        Err(refused(path))
    }
    async fn set_attributes(&self, path: &Path, _change: SetAttributes) -> rc_vfs::Result<()> {
        Err(refused(path))
    }
}

#[async_trait]
impl Vfs for Drain {
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    async fn list(&self, _dir: &Path) -> rc_vfs::Result<Vec<Entry>> {
        Ok(Vec::new())
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        only(path, "/", entry("/", EntryKind::Dir, 0))
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        Err(refused(path))
    }
    async fn open_write(&self, _path: &Path, _mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        Ok(Box::new(Counter(Arc::clone(&self.written))))
    }
    async fn mkdir(&self, _path: &Path, _recursive: bool) -> rc_vfs::Result<()> {
        Ok(())
    }
    async fn rename(&self, _from: &Path, _to: &Path) -> rc_vfs::Result<()> {
        Ok(())
    }
    async fn remove_file(&self, _path: &Path) -> rc_vfs::Result<()> {
        Ok(())
    }
    async fn remove_dir(&self, _path: &Path, _recursive: bool) -> rc_vfs::Result<()> {
        Ok(())
    }
    async fn set_mtime(&self, _path: &Path, _modified: SystemTime) -> rc_vfs::Result<()> {
        Ok(())
    }
    async fn set_attributes(&self, _path: &Path, _change: SetAttributes) -> rc_vfs::Result<()> {
        Ok(())
    }
}

/// The peak resident size of this process in bytes; `None` where `/proc` does not say.
fn peak_resident() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// The copies of this binary, one after the other even where its tests share a process.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Copies the 4 GiB file from `source` to `server` and checks how far the peak grew.
async fn copy_4_gib(source: BackendId, server: BackendId) {
    let _alone = ONE_AT_A_TIME.lock().await;
    let sink: EventSink = Arc::new(|_| {});
    let queue = Queue::new(QueueConfig::default(), sink);
    let written = Arc::new(AtomicU64::new(0));
    queue.register(source.clone(), Backend::new(Arc::new(Endless)));
    let drain = Drain {
        written: Arc::clone(&written),
    };
    queue.register(server.clone(), Backend::new(Arc::new(drain)));
    let spec = JobSpec {
        kind: JobKind::Copy {
            sources: vec![VfsPath {
                backend: source,
                path: PathBuf::from("/big.bin"),
            }],
            target: VfsPath {
                backend: server,
                path: PathBuf::from("/"),
            },
        },
        options: JobOptions::default(),
    };

    let before = peak_resident();
    let id = queue.submit(spec).expect("accepted").id;
    let done = tokio::time::timeout(Duration::from_secs(600), queue.wait(id))
        .await
        .expect("the copy ends in time")
        .expect("known");
    let after = peak_resident();

    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(written.load(Ordering::Relaxed), FILE, "every byte went");
    assert_eq!(done.progress.bytes_done, FILE);
    if let (Some(before), Some(after)) = (before, after) {
        let grown = after.saturating_sub(before);
        assert!(
            grown <= LIMIT,
            "the peak grew by {} MiB copying 4 GiB (limit {} MiB)",
            grown >> 20,
            LIMIT >> 20
        );
    }
}

#[tokio::test]
async fn a_4_gib_file_goes_to_a_server_with_constant_memory() {
    copy_4_gib(BackendId::Memory(1), BackendId::Sftp { connection_id: 1 }).await;
}

#[tokio::test]
async fn a_4_gib_file_goes_from_one_server_to_another_with_constant_memory() {
    copy_4_gib(
        BackendId::Sftp { connection_id: 1 },
        BackendId::Sftp { connection_id: 2 },
    )
    .await;
}
