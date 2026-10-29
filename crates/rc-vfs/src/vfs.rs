//! The trait every backend implements.

use std::{path::Path, time::SystemTime};

use async_trait::async_trait;
use tokio::{
    io::{AsyncRead, AsyncSeek, AsyncWrite},
    sync::mpsc::Sender,
};

use crate::{
    Capabilities, Details, Entry, LinkKind, PropertyCapabilities, PropertyChange, Result,
    SetAttributes, VfsError,
};

/// [`Vfs::rename_no_replace`] for a backend that cannot refuse in the rename itself: `to` is
/// looked at, then renamed onto when nothing is there.
pub(crate) async fn rename_after_look<V: Vfs + ?Sized>(
    vfs: &V,
    from: &Path,
    to: &Path,
) -> Result<()> {
    match vfs.stat(to).await {
        Ok(_) => Err(VfsError::AlreadyExists {
            path: to.to_path_buf(),
        }),
        Err(VfsError::NotFound { .. }) => vfs.rename(from, to).await,
        Err(error) => Err(error),
    }
}

/// A file's contents as a stream.
pub type ReadStream = Box<dyn AsyncRead + Send + Unpin>;

/// What [`SeekStream`] is: a reader that can also seek.
pub trait ReadSeek: AsyncRead + AsyncSeek + Send + Unpin {}

impl<T: AsyncRead + AsyncSeek + Send + Unpin + ?Sized> ReadSeek for T {}

/// A file's contents with random access, for formats that read out of order (zip, 7z).
pub type SeekStream = Box<dyn ReadSeek>;

/// A file being written. Call `shutdown()` (`tokio::io::AsyncWriteExt`) before dropping it:
/// only then are the last bytes guaranteed to have reached the backend, and its error is the
/// one that reports a failed write.
pub type WriteStream = Box<dyn AsyncWrite + Send + Unpin>;

/// How [`Vfs::open_write`] treats a file that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Create the file; fail with `AlreadyExists` when it exists. Conflicts surface here.
    CreateNew,
    /// Create the file, or empty an existing one.
    Truncate,
    /// Create the file, or continue at the end of an existing one (resuming a transfer).
    Append,
}

/// One file system. Object safe: the engine holds backends as `Arc<dyn Vfs>`.
///
/// Every path is a path of this backend (the `path` of a [`crate::VfsPath`] whose backend is
/// this one). Errors are typed by kind ([`crate::VfsError`]), never by backend.
#[async_trait]
pub trait Vfs: Send + Sync {
    /// What this backend can do.
    fn capabilities(&self) -> Capabilities;

    /// The entries of `dir`, without `.` and `..`, in no particular order.
    async fn list(&self, dir: &Path) -> Result<Vec<Entry>>;

    /// The entries of `dir` as [`Vfs::list`] has them, sent to `batches` as they arrive, so a
    /// panel shows the first ones of a long listing before its end (RC-060-02). Once the receiver
    /// is gone the listing stops early, without an error. The default sends the whole
    /// [`Vfs::list`] at once; a backend that reads a directory in blocks (SFTP) sends each block.
    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
        let entries = self.list(dir).await?;
        // A receiver that is gone wants nothing more.
        let _ = batches.send(entries).await;
        Ok(())
    }

    /// The entry at `path`. A symbolic link is described as the link, not followed.
    async fn stat(&self, path: &Path) -> Result<Entry>;

    /// Opens a file for reading from its start.
    async fn open_read(&self, path: &Path) -> Result<ReadStream>;

    /// Opens a file for reading from byte `offset`, to resume an interrupted transfer
    /// (RC-060-07). The default reads from the start only; any other offset is `Unsupported`, and
    /// the caller starts the file over.
    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        if offset == 0 {
            return self.open_read(path).await;
        }
        Err(VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: "reading from an offset",
        })
    }

    /// Opens a file for reading with seeking, for formats that read out of order (zip and 7z
    /// archives). A backend that can only stream keeps this default: `Unsupported`.
    async fn open_read_seekable(&self, path: &Path) -> Result<SeekStream> {
        Err(VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: "seeking in a file",
        })
    }

    /// Opens a file for writing; `mode` decides what happens to an existing one.
    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream>;

    /// Creates a directory; `recursive` creates missing parents too and accepts one that
    /// already exists.
    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()>;

    /// Renames or moves an entry within this backend. An existing file at `to` is replaced
    /// where the backend does so (the local one does); callers that must not overwrite use
    /// [`Vfs::rename_no_replace`].
    async fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Renames like [`Vfs::rename`], but never onto an existing entry: one at `to` is
    /// `AlreadyExists`, and nothing moves. By default `to` is looked at first, so an entry that
    /// appears between the look and the rename can still be replaced; the local backend
    /// overrides it with the file system's own rename that refuses (RC-080-56).
    async fn rename_no_replace(&self, from: &Path, to: &Path) -> Result<()> {
        rename_after_look(self, from, to).await
    }

    /// Removes a file or a symbolic link (never its target).
    async fn remove_file(&self, path: &Path) -> Result<()>;

    /// Removes a directory; `recursive` removes its contents too, without following links.
    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()>;

    /// Sets the modification time.
    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()>;

    /// Changes permissions or attributes; a field the platform has no notion of fails with
    /// `Unsupported` before anything changes.
    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()>;

    /// Bytes available to the user on the file system holding `path`; `None` when the
    /// backend cannot tell.
    async fn free_space(&self, _path: &Path) -> Result<Option<u64>> {
        Ok(None)
    }

    /// What the properties dialog can change here; nothing by default.
    fn property_capabilities(&self) -> PropertyCapabilities {
        PropertyCapabilities::NONE
    }

    /// Facts about the entry at `path` beyond its [`Entry`]; nothing known by default.
    async fn details(&self, _path: &Path) -> Result<Details> {
        Ok(Details::default())
    }

    /// [`Vfs::list`] with each entry's [`Details`], in one pass where the backend can: what a
    /// size count needs (bytes on disk, hard links). The default knows no details.
    async fn list_details(&self, dir: &Path) -> Result<Vec<(Entry, Details)>> {
        Ok(self
            .list(dir)
            .await?
            .into_iter()
            .map(|entry| (entry, Details::default()))
            .collect())
    }

    /// Changes attributes and times together. The default does what [`Vfs::set_attributes`]
    /// and [`Vfs::set_mtime`] do and refuses the Windows flags and the creation time, before
    /// anything changes.
    async fn set_properties(&self, path: &Path, change: PropertyChange) -> Result<()> {
        if change.touches_windows_flags() {
            return Err(VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "setting the hidden, system or archive attribute",
            });
        }
        if change.created.is_some() {
            return Err(VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "setting the creation time",
            });
        }
        if change.mode.is_some() || change.readonly.is_some() {
            let attributes = SetAttributes {
                mode: change.mode,
                readonly: change.readonly,
            };
            self.set_attributes(path, attributes).await?;
        }
        if let Some(modified) = change.modified {
            self.set_mtime(path, modified).await?;
        }
        Ok(())
    }

    /// Creates a link of `kind` at `link` that leads to `target`; never replaces an existing
    /// entry. A kind [`Vfs::property_capabilities`] does not list fails with `Unsupported`.
    async fn create_link(&self, link: &Path, _target: &Path, _kind: LinkKind) -> Result<()> {
        Err(VfsError::Unsupported {
            path: link.to_path_buf(),
            operation: "creating links",
        })
    }
}
