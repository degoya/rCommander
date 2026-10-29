//! [`FtpVfs`]: one FTP server as a [`Vfs`].
//!
//! Paths are absolute FTP paths (`/srv/www/index.html`); every call leases a connection from
//! the server's [pool](super::pool). The server's answers are ambiguous - `550` stands for
//! "missing", "not allowed" and "wrong kind" alike - so a refused operation looks at the path
//! (and its parent) before it says which: that is what makes the errors typed by kind, as the
//! `Vfs` contract wants, on any server. An interrupted download resumes with `REST`
//! ([`Vfs::open_read_at`]), an upload with `APPE` (`WriteMode::Append`), RC-060-07.

mod change;
mod inspect;
mod links;

use std::{fmt, path::Path, sync::Arc, time::SystemTime};

use async_trait::async_trait;
use rc_vfs::{
    Capabilities, Entry, ReadStream, Result, SetAttributes, Vfs, VfsError, WriteMode, WriteStream,
};
use tokio::sync::mpsc::Sender;

use super::{
    config::FtpConfig,
    encoding::ftp_path,
    error::{Failure, FtpError},
    pool::{Lease, Pool, Use},
    stream::Download,
};

/// One FTP or FTPS server as a file system.
#[derive(Clone)]
pub struct FtpVfs {
    pool: Arc<Pool>,
    /// The links listings left unresolved and what was learned of them (`links.rs`).
    links: Arc<links::Links>,
}

impl fmt::Debug for FtpVfs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FtpVfs")
            .field("host", &self.pool.config.host)
            .field("port", &self.pool.config.port)
            .field("encryption", &self.pool.config.encryption)
            .finish_non_exhaustive()
    }
}

impl FtpVfs {
    /// Connects and logs in once, which is also the connection test: a wrong password, an
    /// unreachable host or an untrusted certificate fails here with its code.
    ///
    /// # Errors
    ///
    /// The [`FtpError`] that kept the session from coming about.
    pub async fn connect(config: FtpConfig) -> Result<Self, FtpError> {
        Ok(Self {
            pool: Pool::open(config).await?,
            links: Arc::default(),
        })
    }

    /// The directory the login started in (`PWD`), where a panel opens when the connection
    /// names no start directory; `/` when the server named none or something odd.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.pool.home
    }

    /// Whether its listings look up what their links point at
    /// ([`FtpConfig::resolve_links`]).
    #[must_use]
    pub fn resolves_links(&self) -> bool {
        self.pool.config.resolve_links
    }

    /// Ends the session on purpose (`connections.disconnect`): the idle connections say `QUIT`
    /// and close, the keepalive stops, and nothing is kept from now on. Every clone shares the
    /// session, so one still held - a running transfer's - finishes its call and then closes its
    /// connection; a later call logs in for itself alone.
    pub async fn disconnect(&self) {
        self.pool.close().await;
    }

    /// The FTP spelling of a backend path, checked for what this connection can send.
    fn wire_path(&self, path: &Path) -> Result<String> {
        ftp_path(path, self.pool.wire)
    }

    /// A connection for a command or a listing: the one they share.
    async fn lease(&self, path: &Path) -> Result<Lease> {
        self.pool
            .lease(Use::Command)
            .await
            .map_err(|error| error.into_vfs(path))
    }

    /// A connection for a download or an upload, once fewer than the limit run.
    async fn transfer_lease(&self, path: &Path) -> Result<Lease> {
        self.pool
            .lease(Use::Transfer)
            .await
            .map_err(|error| error.into_vfs(path))
    }

    /// Why reading `path` failed: missing, a directory, or what the server said.
    async fn explain_read(&self, path: &Path, failure: Failure) -> VfsError {
        match self.probe(path).await {
            Ok(None) => not_found(path),
            Ok(Some(entry)) if entry.is_dir_like() => VfsError::IsADirectory {
                path: path.to_path_buf(),
            },
            Ok(Some(_)) | Err(_) => failure.into_vfs(path),
        }
    }
}

fn not_found(path: &Path) -> VfsError {
    VfsError::NotFound {
        path: path.to_path_buf(),
    }
}

#[async_trait]
impl Vfs for FtpVfs {
    fn capabilities(&self) -> Capabilities {
        let info = &self.pool.info;
        Capabilities {
            rename_across_dirs: true,
            symlinks: true,
            case_sensitive: info.case_sensitive,
            unix_mode: info.unix,
            set_mtime: info.mfmt,
            free_space: false,
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        self.list_dir(dir).await
    }

    /// The entries as they arrive, the links last (RC-080-46).
    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
        self.list_dir_batches(dir, batches).await
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        self.stat_path(path).await
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        self.open_read_at(path, 0).await
    }

    /// From `offset` on (`REST`), to resume an interrupted download (RC-060-07). A server that
    /// refuses `REST` gives `unsupported`, and the copy starts the file over: a file is never
    /// spliced from a read the server did not start where it was asked to.
    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        let wire = self.wire_path(path)?;
        let mut lease = self.transfer_lease(path).await?;
        if offset > 0
            && let Err(failure) = lease.rest(offset).await
        {
            return Err(failure.into_vfs(path));
        }
        match lease.retr(&wire).await {
            Ok(transfer) => Ok(Box::new(Download::new(transfer, lease))),
            Err(failure) => {
                if offset > 0 {
                    // A `REST` the failed `RETR` did not consume must not reach the next one.
                    lease.discard();
                }
                drop(lease);
                Err(self.explain_read(path, failure).await)
            }
        }
    }

    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        self.open_upload(path, mode).await
    }

    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        if recursive {
            self.mkdir_all(path).await
        } else {
            self.mkdir_one(path).await
        }
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.rename_entry(from, to).await
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        self.remove_one_file(path).await
    }

    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        if recursive {
            self.remove_tree(path).await
        } else {
            self.remove_empty_dir(path).await
        }
    }

    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()> {
        self.set_modified(path, modified).await
    }

    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        self.change_attributes(path, change).await
    }
}

#[cfg(test)]
#[path = "vfs_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "vfs_transfer_tests.rs"]
mod transfer_tests;

#[cfg(test)]
#[path = "vfs_refusal_tests.rs"]
mod refusal_tests;
