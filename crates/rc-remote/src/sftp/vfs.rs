//! [`SftpVfs`]: one SSH server's file system as a [`Vfs`].

use std::{
    fmt,
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use async_trait::async_trait;
use rc_vfs::{
    Capabilities, Entry, ReadStream, Result, SeekStream, SetAttributes, Vfs, VfsError, WriteMode,
    WriteStream,
};
use russh::ChannelMsg;
use russh_sftp::{
    client::{Config, RawSftpSession, SftpSession, error::Error, fs::File},
    protocol::FileAttributes,
};
use tokio::{
    io::AsyncSeekExt,
    sync::{mpsc::Sender, watch},
    time::timeout,
};

use super::{
    SftpLimits,
    error::{SftpError, into_vfs},
    path::wire_path,
    stream::TransferFile,
};
use crate::ssh::SshChannel;

/// How many reads of one file are on their way at once; with 256 KiB each, 2 MiB in flight is
/// what a slow link must deliver within one request timeout.
const CONCURRENT_READS: usize = 8;
/// How many writes of one file are on their way at once (RC-060-05): 32 KiB each, as OpenSSH's
/// `sftp` sends them, so an upload too keeps 2 MiB in flight and never more.
const CONCURRENT_WRITES: usize = 64;

/// One SSH server's file system over SFTP.
pub struct SftpVfs {
    /// Listings, `stat` and the changes by name.
    pub(super) raw: Arc<RawSftpSession>,
    /// The file streams.
    files: SftpSession,
    home: PathBuf,
    pub(super) limits: SftpLimits,
    /// Set once a call or a file stream found the session gone.
    lost: Arc<AtomicBool>,
    /// The SSH session's end ([`crate::SshSession::closed_signal`]), when the caller gave it.
    gone: Option<watch::Receiver<bool>>,
    /// What the caller keeps alive as long as the backend: the engine's channel lease.
    _held: Option<Box<dyn Send + Sync>>,
}

impl fmt::Debug for SftpVfs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SftpVfs")
            .field("home", &self.home)
            .field("lost", &self.is_closed())
            .finish_non_exhaustive()
    }
}

impl SftpVfs {
    /// Starts the `sftp` subsystem on `names` and `files`, two fresh session channels of one
    /// SSH session, and reads the home directory.
    ///
    /// # Errors
    ///
    /// The server refuses the subsystem (`sftp_unavailable`), does not answer in time
    /// (`sftp_timeout`), closes the channel (`connection_lost`) or does not speak SFTP
    /// (`sftp_protocol`).
    pub async fn connect(
        names: SshChannel,
        files: SshChannel,
        limits: SftpLimits,
    ) -> Result<Self, SftpError> {
        let config = Config {
            request_timeout_secs: limits.request_secs(),
            max_concurrent_reads: CONCURRENT_READS,
            max_concurrent_writes: CONCURRENT_WRITES,
            ..Config::default()
        };
        let names = subsystem(names, &limits).await?;
        let files = subsystem(files, &limits).await?;
        let raw = RawSftpSession::new_with_config(names, config.clone());
        let secs = limits.request_secs();
        raw.init()
            .await
            .map_err(|error| session_failure(&error, secs))?;
        let files = SftpSession::new_with_config(files, config)
            .await
            .map_err(|error| session_failure(&error, secs))?;
        let home = raw
            .realpath(".")
            .await
            .ok()
            .and_then(|name| name.files.into_iter().next());
        let home = crate::wire::home(home.as_ref().map(|file| file.filename.as_str()));
        Ok(Self {
            raw: Arc::new(raw),
            files,
            home,
            limits,
            lost: Arc::new(AtomicBool::new(false)),
            gone: None,
            _held: None,
        })
    }

    /// Keeps `held` alive as long as this backend: the lease of the channels it runs on.
    #[must_use]
    pub fn holding(mut self, held: Box<dyn Send + Sync>) -> Self {
        self._held = Some(held);
        self
    }

    /// Watches `gone`, the end of the SSH session the channels run on: once the session is gone
    /// the backend is closed without a call having to find out, and a listing's request on its
    /// way fails at once as `connection_lost` instead of waiting out its timeout (the SFTP
    /// client answers a request in flight only by its timeout once the channel ended).
    #[must_use]
    pub fn ending_with(mut self, gone: watch::Receiver<bool>) -> Self {
        self.gone = Some(gone);
        self
    }

    /// `request`, or - the session ending while it waits - a lost connection at once.
    pub(super) async fn unless_gone<T>(
        &self,
        request: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        let Some(gone) = &self.gone else {
            return request.await;
        };
        let mut gone = gone.clone();
        tokio::select! {
            answer = request => answer,
            // An error means the session's handler is gone, which is the same.
            _ = gone.wait_for(|gone| *gone) => {
                Err(Error::UnexpectedBehavior("the SSH session ended".to_owned()))
            }
        }
    }

    /// The home directory the server named (`realpath(".")`), where a panel starts; `/` when
    /// it named none that reads.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Whether a call found the session gone or stalled, or the session ended: the next use
    /// needs a new backend.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.lost.load(Ordering::SeqCst) || self.gone.as_ref().is_some_and(|gone| *gone.borrow())
    }

    /// `file` as a transfer streams it: a lost session mid-stream is `connection_lost` and closes
    /// the backend (`stream.rs`).
    pub(super) fn transfer_file(&self, file: File, path: &Path) -> TransferFile {
        TransferFile::new(
            file,
            path.to_path_buf(),
            Arc::clone(&self.lost),
            self.limits.request_secs(),
        )
    }

    /// The `VfsError` of a failed request on `path`; a lost or stalled session marks the backend
    /// closed.
    pub(super) fn fail(&self, error: Error, path: &Path) -> VfsError {
        let secs = self.limits.request_secs();
        if matches!(
            SftpError::of_client(&error, secs),
            Some(SftpError::ConnectionLost | SftpError::Timeout { .. })
        ) {
            self.lost.store(true, Ordering::SeqCst);
        }
        into_vfs(error, path, secs)
    }

    /// What is at `path`, a link followed: `None` when nothing is there (or a dangling link).
    pub(super) async fn probe(&self, path: &Path) -> Result<Option<FileAttributes>> {
        self.look(path, false).await
    }

    /// What is at `path`, a link as the link.
    pub(super) async fn probe_link(&self, path: &Path) -> Result<Option<FileAttributes>> {
        self.look(path, true).await
    }

    async fn look(&self, path: &Path, link: bool) -> Result<Option<FileAttributes>> {
        let wire = wire_path(path)?;
        let answer = if link {
            self.raw.lstat(wire).await
        } else {
            self.raw.stat(wire).await
        };
        match answer {
            Ok(attrs) => Ok(Some(attrs.attrs)),
            Err(error) if super::error::is_no_such_file(&error) => Ok(None),
            Err(error) => Err(self.fail(error, path)),
        }
    }

    /// A regular file opened for reading; a directory is refused before it is opened (a server
    /// opens one without complaint and fails the first read).
    async fn open_file(&self, path: &Path) -> Result<File> {
        let wire = wire_path(path)?;
        match self.probe(path).await? {
            None => {
                return Err(VfsError::NotFound {
                    path: path.to_path_buf(),
                });
            }
            Some(attrs) if attrs.is_dir() => {
                return Err(VfsError::IsADirectory {
                    path: path.to_path_buf(),
                });
            }
            Some(_) => {}
        }
        self.files
            .open(wire)
            .await
            .map_err(|error| self.fail(error, path))
    }

    pub(super) async fn open_for_writing(
        &self,
        wire: String,
        flags: russh_sftp::protocol::OpenFlags,
    ) -> std::result::Result<File, Error> {
        self.files.open_with_flags(wire, flags).await
    }
}

/// Starts the `sftp` subsystem on `channel` and waits for the server's yes.
async fn subsystem(
    mut channel: SshChannel,
    limits: &SftpLimits,
) -> Result<russh::ChannelStream<russh::client::Msg>, SftpError> {
    let seconds = limits.request_secs();
    let answer = timeout(limits.request_timeout, async {
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|_| SftpError::ConnectionLost)?;
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => return Ok(()),
                Some(ChannelMsg::Failure) => return Err(SftpError::Unavailable),
                Some(ChannelMsg::Close | ChannelMsg::Eof) | None => {
                    return Err(SftpError::ConnectionLost);
                }
                Some(_) => {}
            }
        }
    })
    .await
    .map_err(|_| SftpError::Timeout { seconds })?;
    answer?;
    Ok(channel.into_stream())
}

/// The session failure of a client error during the setup; any refusal there means the server
/// does not speak SFTP as it should.
fn session_failure(error: &Error, seconds: u64) -> SftpError {
    SftpError::of_client(error, seconds).unwrap_or(SftpError::Protocol {
        reason: "the SFTP version exchange failed",
    })
}

#[async_trait]
impl Vfs for SftpVfs {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: true,
            symlinks: true,
            case_sensitive: true,
            unix_mode: true,
            set_mtime: true,
            free_space: false,
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        self.list_dir(dir, true).await
    }

    /// Each `READDIR` answer as it arrives (RC-060-02).
    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
        self.list_blocks(dir, true, batches).await
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        self.stat_path(path).await
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        self.open_read_at(path, 0).await
    }

    /// From `offset` on, to resume an interrupted download (RC-060-07).
    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        let mut file = self.open_file(path).await?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|error| VfsError::from_io(error, path))?;
        }
        Ok(Box::new(self.transfer_file(file, path)))
    }

    async fn open_read_seekable(&self, path: &Path) -> Result<SeekStream> {
        Ok(Box::new(self.open_file(path).await?))
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
