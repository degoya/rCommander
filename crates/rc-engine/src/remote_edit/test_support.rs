//! What the remote edits' tests share: an engine with a server - an FTP backend over memory that
//! keeps Unix modes and records its writes and renames, and on request refuses a new mode, stops
//! answering a write or takes it slowly -, a root of their own for the edit
//! folders, and no watch task (a test reads the copies itself, [`Engine::poll_copies`]).

use std::{
    collections::HashMap,
    future::Future as _,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use rc_ops::Backend;
use rc_vfs::{
    Attributes, BackendId, Capabilities, Entry, EntryKind, MemoryFs, SetAttributes, TargetKind,
    Vfs, VfsPath, WriteMode, WriteStream,
};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt as _;

use super::{RemoteEditId, RemoteEditInfo};
use crate::recover::LockOrRecover as _;
use crate::{Engine, test_support};

/// The server's backend.
pub(crate) const SERVER: BackendId = BackendId::Ftp { connection_id: 7 };

/// A server over memory with Unix modes, recording the paths it was written to and renamed.
#[derive(Default)]
pub(crate) struct Server {
    inner: MemoryFs,
    modes: Mutex<HashMap<PathBuf, u32>>,
    /// Links by path, each with its stored target: the memory backend has none of its own.
    links: Mutex<HashMap<PathBuf, PathBuf>>,
    pub writes: Mutex<Vec<PathBuf>>,
    pub renames: Mutex<Vec<(PathBuf, PathBuf)>>,
    /// A new mode is refused, as a server refuses the `chmod` of a file that is someone else's.
    pub refuse_modes: AtomicBool,
    /// How often a mode was asked for.
    pub mode_calls: AtomicUsize,
    /// A file opened to write takes no byte and never answers.
    pub stall_writes: AtomicBool,
    /// A stalled write was let go.
    pub let_go: Arc<AtomicBool>,
    /// A file opened to write takes one byte every few milliseconds.
    pub slow_writes: AtomicBool,
}

/// A write that moves, slowly: one byte after each [`Slow::STEP`].
struct Slow {
    inner: WriteStream,
    wait: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl Slow {
    const STEP: Duration = Duration::from_millis(5);
}

impl tokio::io::AsyncWrite for Slow {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let wait = self
            .wait
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(Self::STEP)));
        if wait.as_mut().poll(context).is_pending() {
            return Poll::Pending;
        }
        self.wait = None;
        let one = &bytes[..bytes.len().min(1)];
        Pin::new(&mut self.inner).poll_write(context, one)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

/// A write the server never answers; it notes when it is let go.
struct Stalled {
    let_go: Arc<AtomicBool>,
}

impl tokio::io::AsyncWrite for Stalled {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        _bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Pending
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

impl Drop for Stalled {
    fn drop(&mut self) {
        self.let_go.store(true, Ordering::SeqCst);
    }
}

impl Server {
    /// Writes `text` to `path`, its folders made first.
    pub(crate) async fn put(&self, path: &str, text: &str) {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            self.inner.mkdir(parent, true).await.expect("mkdir");
        }
        let mut file = self
            .inner
            .open_write(path, WriteMode::Truncate)
            .await
            .expect("open");
        file.write_all(text.as_bytes()).await.expect("write");
        file.shutdown().await.expect("close");
    }

    /// What `path` holds.
    pub(crate) async fn text(&self, path: &str) -> String {
        let mut reader = self.inner.open_read(Path::new(path)).await.expect("open");
        let mut text = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut text)
            .await
            .expect("read");
        text
    }

    /// Removes the file `path`.
    pub(crate) async fn remove(&self, path: &str) {
        self.inner
            .remove_file(Path::new(path))
            .await
            .expect("removed");
    }

    pub(crate) fn set_mode(&self, path: &str, mode: u32) {
        self.modes
            .lock_or_recover("server.modes")
            .insert(path.into(), mode);
    }

    /// A link at `path` whose stored target is `target`.
    pub(crate) fn link(&self, path: &str, target: &str) {
        self.links
            .lock_or_recover("server.links")
            .insert(path.into(), target.into());
    }

    pub(crate) fn mode(&self, path: &str) -> Option<u32> {
        self.modes
            .lock_or_recover("server.modes")
            .get(Path::new(path))
            .copied()
    }
}

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Server {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            unix_mode: true,
            ..self.inner.capabilities()
        }
    }

    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        let link = self
            .links
            .lock_or_recover("server.links")
            .get(path)
            .cloned();
        if let Some(target) = link {
            let mut entry = self.inner.stat(Path::new("/")).await?;
            entry.name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            entry.kind = EntryKind::Symlink {
                target,
                target_kind: Some(TargetKind::File),
            };
            return Ok(entry);
        }
        let mut entry = self.inner.stat(path).await?;
        if let Some(mode) = self.modes.lock_or_recover("server.modes").get(path) {
            entry.attributes = Attributes::from_mode(*mode);
        }
        Ok(entry)
    }

    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        self.writes
            .lock_or_recover("server.writes")
            .push(path.to_path_buf());
        if self.stall_writes.load(Ordering::SeqCst) {
            return Ok(Box::new(Stalled {
                let_go: Arc::clone(&self.let_go),
            }));
        }
        if self.slow_writes.load(Ordering::SeqCst) {
            return Ok(Box::new(Slow {
                inner: self.inner.open_write(path, mode).await?,
                wait: None,
            }));
        }
        self.inner.open_write(path, mode).await
    }

    /// A rename leaves the new file the mode the part had: none of the old file's.
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        self.renames
            .lock_or_recover("server.renames")
            .push((from.to_path_buf(), to.to_path_buf()));
        {
            let mut modes = self.modes.lock_or_recover("server.modes");
            modes.remove(to);
            if let Some(mode) = modes.remove(from) {
                modes.insert(to.to_path_buf(), mode);
            }
        }
        self.inner.rename(from, to).await
    }

    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> rc_vfs::Result<()> {
        match change.mode {
            Some(_) if self.refuse_modes.load(Ordering::SeqCst) => {
                self.mode_calls.fetch_add(1, Ordering::SeqCst);
                Err(rc_vfs::VfsError::PermissionDenied {
                    path: path.to_path_buf(),
                })
            }
            Some(mode) => {
                self.mode_calls.fetch_add(1, Ordering::SeqCst);
                self.modes
                    .lock_or_recover("server.modes")
                    .insert(path.to_path_buf(), mode);
                Ok(())
            }
            None => self.inner.set_attributes(path, change).await,
        }
    }
}

/// An engine with a database whose left panel shows the server's `/etc`, the edit folders under
/// a root of the test's own.
pub(crate) struct Rig {
    pub engine: Engine,
    pub server: Arc<Server>,
    pub edits: TempDir,
    _panels: TempDir,
}

impl Rig {
    /// A rig whose server holds `files` (path, text).
    pub(crate) async fn new(files: &[(&str, &str)]) -> Self {
        let (panels, engine) = test_support::engine_with_db(&[("local.txt", "x")]).await;
        let server = Arc::new(Server::default());
        server.put("/etc/.keep", "").await;
        for (path, text) in files {
            server.put(path, text).await;
        }
        let vfs: Arc<dyn Vfs> = server.clone();
        engine.inner.remotes.open().insert(SERVER, Arc::clone(&vfs));
        engine.ops().register(SERVER, Backend::new(vfs));
        let edits = tempfile::tempdir().expect("temp dir");
        {
            let mut desk = engine.edits();
            desk.root = Some(edits.path().to_path_buf());
            // The tests read the copies themselves.
            desk.polling = true;
        }
        Self {
            engine,
            server,
            edits,
            _panels: panels,
        }
    }

    /// The server path `path` as a location.
    pub(crate) fn at(path: &str) -> VfsPath {
        VfsPath {
            backend: SERVER,
            path: path.into(),
        }
    }

    /// The edits now.
    pub(crate) fn list(&self) -> Vec<RemoteEditInfo> {
        self.engine.remote_edits()
    }

    /// The one edit of `name`.
    pub(crate) fn edit(&self, name: &str) -> RemoteEditInfo {
        self.list()
            .into_iter()
            .find(|edit| edit.name == name)
            .unwrap_or_else(|| panic!("no edit of {name}"))
    }

    /// The local copy of `id`.
    pub(crate) fn copy(&self, id: RemoteEditId) -> PathBuf {
        self.engine
            .edits()
            .edits
            .get(&id)
            .map(|edit| edit.file.clone())
            .expect("an edit")
    }

    /// Saves `text` into the copy of `id`, as an editor would, and lets two reads see it.
    pub(crate) async fn save(&self, id: RemoteEditId, text: &str) {
        let copy = self.copy(id);
        // A new time as well as a new size, whatever the file system's clock resolution.
        let time = std::fs::metadata(&copy)
            .and_then(|meta| meta.modified())
            .expect("a time")
            + Duration::from_secs(2);
        std::fs::write(&copy, text).expect("save");
        std::fs::File::options()
            .write(true)
            .open(&copy)
            .and_then(|file| file.set_modified(time))
            .expect("time");
        assert!(self.engine.poll_copies().await);
        assert!(self.engine.poll_copies().await);
    }

    /// Waits until the server's `path` holds `text`.
    pub(crate) async fn until_server(&self, path: &str, text: &str) {
        for _ in 0..500 {
            if self.server.text(path).await == text {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{path} never held {text:?}");
    }

    /// Waits until the edit of `name` satisfies `wanted`.
    pub(crate) async fn until(&self, name: &str, wanted: impl Fn(&RemoteEditInfo) -> bool) {
        for _ in 0..500 {
            if self
                .list()
                .iter()
                .any(|edit| edit.name == name && wanted(edit))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the edit of {name} never got there: {:?}", self.list());
    }
}
