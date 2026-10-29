//! What the viewer's tests share: files to view and backends to view them on.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use rc_vfs::{LocalFs, Vfs};

use super::FileWindow;

/// A local file of `len` bytes whose byte at `i` is `i % 251`, so every window is distinct.
pub(crate) fn numbered(dir: &Path, len: usize) -> PathBuf {
    let path = dir.join("numbered.bin");
    let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
    fs::write(&path, bytes).expect("write");
    path
}

pub(crate) fn local() -> Arc<dyn Vfs> {
    Arc::new(LocalFs::new())
}

/// The local backend without seeking, as an archive member or a plain stream would be.
pub(crate) struct StreamOnly(pub LocalFs);

impl rc_vfs::testing::Forward for StreamOnly {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
}

/// The local backend that cannot seek but starts a read at an offset, as FTP does with `REST`;
/// a read from the start is refused, so a test sees no offset reached by skipping.
pub(crate) struct Resumable(pub LocalFs);

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Resumable {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }

    async fn open_read(&self, path: &Path) -> rc_vfs::Result<rc_vfs::ReadStream> {
        Err(rc_vfs::VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: "reading from the start in this test",
        })
    }

    async fn open_read_at(&self, path: &Path, offset: u64) -> rc_vfs::Result<rc_vfs::ReadStream> {
        self.0.open_read_at(path, offset).await
    }
}

/// The local backend that cannot seek but starts a read at an offset, as FTP does with `REST`,
/// counting the reads it opens: on FTP each is a new data connection (RC-080-57).
#[derive(Default)]
pub(crate) struct Connections {
    local: LocalFs,
    opened: AtomicUsize,
}

impl Connections {
    /// The reads opened so far, from the start or at an offset.
    pub(crate) fn opened(&self) -> usize {
        self.opened.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for Connections {
    fn inner(&self) -> &dyn Vfs {
        &self.local
    }

    async fn open_read(&self, path: &Path) -> rc_vfs::Result<rc_vfs::ReadStream> {
        self.opened.fetch_add(1, Ordering::Relaxed);
        self.local.open_read(path).await
    }

    async fn open_read_at(&self, path: &Path, offset: u64) -> rc_vfs::Result<rc_vfs::ReadStream> {
        self.opened.fetch_add(1, Ordering::Relaxed);
        self.local.open_read_at(path, offset).await
    }
}

/// `bytes` as the file `name` in `dir`.
pub(crate) fn file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write");
    path
}

/// `path` opened on the local backend.
pub(crate) async fn open(path: &Path) -> FileWindow {
    FileWindow::open(local(), path).await.expect("open")
}
