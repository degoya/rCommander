//! What the tests share: a local file system in a temporary folder, and one that cancels a run
//! half way.

use std::{
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use rc_vfs::{LocalFs, ReadStream, Vfs};
use tempfile::TempDir;
use tokio::io::{AsyncRead, ReadBuf};

/// A temporary folder and the local file system.
pub(crate) fn local() -> (TempDir, Arc<dyn Vfs>) {
    let root = tempfile::tempdir().expect("temp dir");
    (root, Arc::new(LocalFs::new()))
}

/// Writes `contents` to `name` under `dir`.
pub(crate) fn write(dir: &Path, name: &str, contents: &[u8]) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, contents).expect("write");
}

/// `len` bytes of a pattern that does not repeat within a part.
pub(crate) fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from((index * 31 + index / 251) % 256).expect("below 256"))
        .collect()
}

/// The names in `dir`, sorted.
pub(crate) fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The local file system, except that every stream it opens for reading cancels `control`
/// once `after` bytes went through it: a cancel in the middle of a run, without timing.
pub(crate) struct Tripwire {
    pub(crate) inner: LocalFs,
    pub(crate) control: crate::Control,
    pub(crate) after: u64,
}

struct TripReader {
    inner: ReadStream,
    control: crate::Control,
    left: u64,
}

impl AsyncRead for TripReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(cx, buf);
        let read = (buf.filled().len() - before) as u64;
        self.left = self.left.saturating_sub(read);
        if self.left == 0 {
            self.control.cancel();
        }
        polled
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Tripwire {
    fn inner(&self) -> &dyn Vfs {
        &self.inner
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        Ok(Box::new(TripReader {
            inner: self.inner.open_read(path).await?,
            control: self.control.clone(),
            left: self.after,
        }))
    }
}
