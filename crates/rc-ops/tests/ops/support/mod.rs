//! What the queue tests share: a queue that records its events and lets a test act on them,
//! in-memory backends, a backend that corrupts what it writes and one that lies in its
//! listings.

#![allow(dead_code, reason = "each test file uses its own part")]

pub mod flaky;
pub mod servers;
pub mod slow;

use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use rc_ops::{
    Backend, EventSink, JobChange, JobEvent, JobId, JobSnapshot, JobSpec, OpsError, Queue,
    QueueConfig, Trash,
};
use rc_vfs::{
    Attributes, BackendId, Capabilities, Entry, EntryKind, MemoryFs, ReadStream, Vfs, VfsPath,
    WriteMode, WriteStream, conformance,
};
use tokio::io::AsyncWrite;

/// Acts on an event, from inside the job's task (the queue holds no lock then).
pub type Hook = Box<dyn FnMut(&Queue, &JobEvent) + Send>;

/// A queue with a 4 KiB buffer and no progress throttle, recording every event.
pub struct Harness {
    pub queue: Queue,
    events: Arc<Mutex<Vec<JobEvent>>>,
    hook: Arc<Mutex<Option<Hook>>>,
}

impl Harness {
    pub fn start() -> Self {
        let events: Arc<Mutex<Vec<JobEvent>>> = Arc::default();
        let hook: Arc<Mutex<Option<Hook>>> = Arc::default();
        let cell: Arc<OnceLock<Queue>> = Arc::default();
        let sink: EventSink = {
            let (events, hook, cell) = (Arc::clone(&events), Arc::clone(&hook), Arc::clone(&cell));
            Arc::new(move |event: JobEvent| {
                events.lock().expect("events").push(event.clone());
                // A hook that pauses or answers causes events itself: those skip the hook.
                if let (Some(queue), Ok(mut hook)) = (cell.get(), hook.try_lock())
                    && let Some(hook) = hook.as_mut()
                {
                    hook(queue, &event);
                }
            })
        };
        let config = QueueConfig {
            buffer_size: 4096,
            progress_interval: Duration::ZERO,
            ..QueueConfig::default()
        };
        let queue = Queue::new(config, sink);
        let _ = cell.set(queue.clone());
        Self {
            queue,
            events,
            hook,
        }
    }

    /// A fresh in-memory backend registered as `Memory(n)`.
    pub fn memory(&self, n: u32) -> MemoryFs {
        let fs = MemoryFs::new();
        self.queue
            .register(BackendId::Memory(n), Backend::new(Arc::new(fs.clone())));
        fs
    }

    /// A fresh in-memory backend registered as `id`: a server's id stands for a server
    /// (RC-060-05: part files, the connection's lane).
    pub fn memory_as(&self, id: BackendId) -> MemoryFs {
        let fs = MemoryFs::new();
        self.queue.register(id, Backend::new(Arc::new(fs.clone())));
        fs
    }

    /// The local file system, without a recycle bin.
    pub fn local(&self) {
        self.queue.register(
            BackendId::Local,
            Backend::new(Arc::new(rc_vfs::LocalFs::new())),
        );
    }

    pub fn on_event(&self, hook: impl FnMut(&Queue, &JobEvent) + Send + 'static) {
        *self.hook.lock().expect("hook") = Some(Box::new(hook));
    }

    /// Every event so far, in order.
    pub fn events(&self) -> Vec<JobEvent> {
        self.events.lock().expect("events").clone()
    }

    /// The changes of one job, in order.
    pub fn changes(&self, job: JobId) -> Vec<JobChange> {
        self.events
            .lock()
            .expect("events")
            .iter()
            .filter(|event| event.job == job)
            .map(|event| event.change.clone())
            .collect()
    }

    /// Submits and waits for the end.
    pub async fn run(&self, spec: JobSpec) -> JobSnapshot {
        let id = self.queue.submit(spec).expect("the job is accepted").id;
        self.finish(id).await
    }

    pub async fn finish(&self, id: JobId) -> JobSnapshot {
        tokio::time::timeout(Duration::from_secs(20), self.queue.wait(id))
            .await
            .expect("the job ends in time")
            .expect("the job is known")
    }
}

/// `path` on `Memory(n)`.
pub fn mem(n: u32, path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Memory(n),
        path: PathBuf::from(path),
    }
}

/// Bytes that differ from buffer to buffer.
pub fn payload(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8 ^ seed).collect()
}

pub async fn write(vfs: &dyn Vfs, path: &str, bytes: &[u8]) {
    if let Some(parent) = Path::new(path).parent() {
        vfs.mkdir(parent, true).await.expect("mkdir");
    }
    conformance::write_file(vfs, Path::new(path), WriteMode::Truncate, bytes).await;
}

pub async fn read(vfs: &dyn Vfs, path: &str) -> Vec<u8> {
    conformance::read_file(vfs, Path::new(path)).await
}

pub async fn exists(vfs: &dyn Vfs, path: &str) -> bool {
    vfs.stat(Path::new(path)).await.is_ok()
}

pub async fn set_mtime(vfs: &dyn Vfs, path: &str, secs: u64) {
    vfs.set_mtime(
        Path::new(path),
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
    )
    .await
    .expect("set mtime");
}

/// `tree.zip` of the archive backend's fixtures with the CRC of `hello.txt` changed in its local
/// header and in the central directory, as `damaged.zip` in `dir`: the data inflates, the
/// checksum does not match, and reading the entry fails at its end.
pub fn damaged_zip(dir: &Path) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join("tree.zip");
    let mut bytes = std::fs::read(fixture).expect("read the fixture");
    let name = b"hello.txt";
    let mut patched = 0;
    for at in 0..bytes.len().saturating_sub(46) {
        let (crc, name_at) = match &bytes[at..at + 4] {
            b"PK\x03\x04" => (at + 14, at + 30),
            b"PK\x01\x02" => (at + 16, at + 46),
            _ => continue,
        };
        if bytes.get(name_at..name_at + name.len()) == Some(name.as_slice()) {
            bytes[crc] ^= 0xff;
            patched += 1;
        }
    }
    assert_eq!(patched, 2, "local header and central directory");
    let path = dir.join("damaged.zip");
    std::fs::write(&path, &bytes).expect("write");
    path
}

/// A backend that flips the first byte of every write: a target that does not hold what it
/// was given.
pub struct Corrupting(pub MemoryFs);

struct CorruptingWriter(WriteStream);

impl AsyncWrite for CorruptingWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut changed = bytes.to_vec();
        if let Some(first) = changed.first_mut() {
            *first ^= 0xFF;
        }
        Pin::new(&mut self.0).poll_write(cx, &changed)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Corrupting {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        Ok(Box::new(CorruptingWriter(
            self.0.open_write(path, mode).await?,
        )))
    }
}

/// The names a [`Lying`] listing adds to every folder: each would lead out of it when joined.
pub const LIES: &[&str] = &["../escape", "/abs"];

/// A hostile server: every listing adds the [`LIES`] as files of four bytes, which it also
/// answers when they are asked for.
pub struct Lying(pub MemoryFs);

impl Lying {
    fn lie(path: &Path) -> bool {
        path.ends_with("escape") || path.ends_with("abs")
    }

    fn entry(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            kind: EntryKind::File,
            size: 4,
            modified: None,
            created: None,
            attributes: Attributes::default(),
            hidden: false,
            owner: None,
        }
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Lying {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn list(&self, dir: &Path) -> rc_vfs::Result<Vec<Entry>> {
        let mut entries = self.0.list(dir).await?;
        entries.extend(LIES.iter().map(|name| Self::entry(name)));
        Ok(entries)
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        if Self::lie(path) {
            return Ok(Self::entry("lie"));
        }
        self.0.stat(path).await
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        if Self::lie(path) {
            return Ok(Box::new(io::Cursor::new(b"evil".to_vec())));
        }
        self.0.open_read(path).await
    }
}

/// A recycle bin inside a [`MemoryFs`]: entries are renamed into `/.bin`.
pub struct BinTrash(pub MemoryFs);

#[async_trait]
impl Trash for BinTrash {
    async fn trash(&self, path: &Path) -> Result<(), OpsError> {
        self.0.mkdir(Path::new("/.bin"), true).await?;
        let name = path.file_name().unwrap_or_default();
        self.0.rename(path, &Path::new("/.bin").join(name)).await?;
        Ok(())
    }
}

/// The renames a [`CaseBlind`] backend was asked for, in order.
pub type Renames = Arc<Mutex<Vec<(PathBuf, PathBuf)>>>;

/// The paths a rename onto fails on a [`CaseBlind`] backend; a test may change them meanwhile.
pub type FailInto = Arc<Mutex<Vec<PathBuf>>>;

/// A backend that says case does not count, as Windows' does, and records its renames; a rename
/// onto one of `fail_into` fails.
pub struct CaseBlind {
    pub fs: MemoryFs,
    pub renames: Renames,
    pub fail_into: FailInto,
}

impl Harness {
    /// A [`CaseBlind`] backend registered as `Memory(n)`: its files and its rename record.
    pub fn case_blind(&self, n: u32, fail_into: Option<&str>) -> (MemoryFs, Renames) {
        let (fs, renames, _fail_into) = self.case_blind_failing(n, fail_into.as_slice());
        (fs, renames)
    }

    /// A [`CaseBlind`] backend registered as `Memory(n)` whose renames onto `fail_into` fail:
    /// its files, its rename record and the failing paths.
    pub fn case_blind_failing(&self, n: u32, fail_into: &[&str]) -> (MemoryFs, Renames, FailInto) {
        let fs = MemoryFs::new();
        let renames = Renames::default();
        let failing: FailInto = Arc::new(Mutex::new(
            fail_into.iter().copied().map(PathBuf::from).collect(),
        ));
        let backend = CaseBlind {
            fs: fs.clone(),
            renames: Arc::clone(&renames),
            fail_into: Arc::clone(&failing),
        };
        self.queue
            .register(BackendId::Memory(n), Backend::new(Arc::new(backend)));
        (fs, renames, failing)
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for CaseBlind {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    fn capabilities(&self) -> Capabilities {
        let mut capabilities = self.fs.capabilities();
        capabilities.case_sensitive = false;
        capabilities
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        self.renames
            .lock()
            .expect("renames")
            .push((from.to_path_buf(), to.to_path_buf()));
        if self
            .fail_into
            .lock()
            .expect("fail into")
            .iter()
            .any(|path| path == to)
        {
            return Err(rc_vfs::VfsError::PermissionDenied {
                path: to.to_path_buf(),
            });
        }
        self.fs.rename(from, to).await
    }
}
