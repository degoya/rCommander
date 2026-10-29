//! Resuming a transfer (RC-060-07): a copy to or from a server - or from disk to disk, RC-080-04 -
//! writes `<name>.rcpart` and keeps it when the connection goes, waits, reconnects and continues
//! from what the part holds.
//!
//! A file continues only when that is safe; otherwise it starts over, and a spliced file never
//! comes about:
//!
//! - the source still has the size and modification time the job saw (a [`FileMark`] across a
//!   restart, the plan within one run);
//! - the part is no longer than the source, and its last [`TAIL_CHECK`] bytes are the source's
//!   bytes at the same place;
//! - the source reads from the offset (`open_read_at`: a seek, SFTP's offset reads, FTP's `REST`)
//!   and the part takes more at its end (`WriteMode::Append`: SFTP's append, FTP's `APPE`) - a
//!   server that refuses either starts the file over.
//!
//! A lost connection (an I/O error of the kinds a broken link has, [`is_connection_lost`]) is
//! retried [`RetryPolicy::attempts`] times with a growing wait, each time on a backend the
//! [`Reconnect`] hook opened again; then the job pauses as resumable, the lost connection in
//! its failures while it waits, and `ops.resume` tries again. A cancel, or any other failure,
//! removes the part, as a cancelled copy always did; a part that cannot be removed is logged.

use std::{io, path::Path, sync::Arc, time::Duration};

use async_trait::async_trait;
use rc_vfs::{BackendId, Vfs, VfsError, WriteMode};
use tokio::io::AsyncReadExt;

use crate::{
    Failure, JobChange, OpsError,
    copy::Moved,
    persist::{FileMark, MARK_MIN_BYTES},
    runner::{Runner, Step},
    staging::{is_remote, open_part, remove_leftover},
    temp_name::part_path,
    walk::Item,
};

/// The bytes before the offset compared between source and part before a file continues.
pub const TAIL_CHECK: u64 = 64 * 1024;
/// Retries of a lost connection before a job pauses, unless configured.
pub const DEFAULT_RETRY_ATTEMPTS: u32 = 3;
/// The first wait before a retry, unless configured; each further one doubles it.
pub const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(5);

/// How a transfer meets a lost connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Retries before the job pauses; 0 pauses at once.
    pub attempts: u32,
    /// The wait before the first retry; it doubles with each one.
    pub delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: DEFAULT_RETRY_ATTEMPTS,
            delay: DEFAULT_RETRY_DELAY,
        }
    }
}

impl RetryPolicy {
    /// The wait before retry `done + 1`: the delay, doubled for each retry before it.
    #[must_use]
    pub fn wait(&self, done: u32) -> Duration {
        self.delay.saturating_mul(1_u32 << done.min(16))
    }
}

/// Opens a server's backend again after its connection was lost: the engine's opens a new
/// session where the old one is gone, and registers the backend with the queue.
#[async_trait]
pub trait Reconnect: Send + Sync {
    /// The backend of `backend` as it works now.
    ///
    /// # Errors
    ///
    /// Why it cannot be had; the retry counts as failed.
    async fn reconnect(&self, backend: &BackendId) -> Result<Arc<dyn Vfs>, OpsError>;
}

/// Whether `error` is a connection that broke - worth a retry, never a reason to remove a part.
/// A share that stopped answering is one too (`drive_timed_out`, RC-080-04).
#[must_use]
pub fn is_connection_lost(error: &OpsError) -> bool {
    if matches!(error, OpsError::Vfs(VfsError::TimedOut { .. })) {
        return true;
    }
    let OpsError::Vfs(VfsError::Io { source, .. }) = error else {
        return false;
    };
    matches!(
        source.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::NotConnected
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::TimedOut
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::NetworkDown
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::HostUnreachable
    )
}

/// Where a file goes and how: its target, how an existing target is met when the part is
/// renamed (`CreateNew` or `Truncate`), and whether its part file is continued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) dest: std::path::PathBuf,
    pub(crate) mode: WriteMode,
    pub(crate) resume: bool,
}

impl Placement {
    pub(crate) fn new(dest: std::path::PathBuf, mode: WriteMode) -> Self {
        Self {
            dest,
            mode,
            resume: false,
        }
    }
}

impl Runner {
    /// A staged file through its part file, retried and continued after a lost connection;
    /// renamed instead where a move stays on one backend.
    pub(crate) async fn copy_resuming(
        &mut self,
        item: &Item,
        placement: &Placement,
    ) -> Result<Moved, Step> {
        if self.rename_ok {
            return self.copy_one(item, &placement.dest, placement.mode).await;
        }
        let part = part_path(&placement.dest);
        self.keep_mark(item, placement).await;
        let start = self.progress.bytes_done;
        let mut resume = placement.resume;
        let mut retries = 0_u32;
        loop {
            self.progress.restart_file(start);
            let attempt = match self.write_part(item, &part, resume).await {
                Ok(()) => self.settle(&part, &placement.dest, placement.mode).await,
                Err(step) => Err(step),
            };
            match attempt {
                Ok(()) => {
                    self.preserve(&item.entry, &placement.dest).await;
                    return Ok(Moved::Copied);
                }
                Err(Step::Fail(error)) if is_connection_lost(&error) => {
                    let mut why = Failure::new(&self.src_path(&item.path), &error);
                    why.target = self.failed_on.take().map(Box::new);
                    if let Err(stop) = self.recover(&mut retries, why).await {
                        // Cancelled while it waited: the part goes, as with any cancel.
                        remove_leftover(self.dst.as_ref(), &part).await;
                        return Err(stop);
                    }
                    resume = true;
                }
                Err(step) => {
                    remove_leftover(self.dst.as_ref(), &part).await;
                    return Err(step);
                }
            }
        }
    }

    /// Writes the part file whole: from the start, or from where it ends when that is safe.
    async fn write_part(&mut self, item: &Item, part: &Path, resume: bool) -> Result<(), Step> {
        self.failed_on = None;
        let mut offset = if resume {
            self.resume_offset(item, part).await?
        } else {
            0
        };
        let mut hasher = blake3::Hasher::new();
        if offset > 0 && self.options.verify {
            // The check after the copy covers the whole file: the part's bytes too.
            self.hash_prefix(&item.path, offset, &mut hasher).await?;
        }
        let pair = self.one_ftp_pair().await?;
        let mut streams = None;
        if offset > 0 {
            streams = self.open_at(item, part, offset).await?;
            if streams.is_none() {
                offset = 0;
                hasher = blake3::Hasher::new();
            }
        }
        let (reader, writer) = match streams {
            Some(streams) => streams,
            None => {
                let reader = self.src.open_read(&item.path).await?;
                let writer = open_part(self.dst.as_ref(), part)
                    .await
                    .map_err(|error| self.target_refused(error, part))?;
                (reader, writer)
            }
        };
        self.progress.count(offset);
        self.tick(false);
        self.land(item, (reader, writer), pair, part, hasher, offset)
            .await
    }

    /// The source from `offset` and the part opened to append; `None` when either side refuses
    /// (a server without `REST` or `APPE`): the file starts over. A lost connection is an error.
    async fn open_at(
        &self,
        item: &Item,
        part: &Path,
        offset: u64,
    ) -> Result<Option<(rc_vfs::ReadStream, rc_vfs::WriteStream)>, Step> {
        let reader = match self.src.open_read_at(&item.path, offset).await {
            Ok(reader) => reader,
            Err(error) => return refused(error),
        };
        match self.dst.open_write(part, WriteMode::Append).await {
            Ok(writer) => Ok(Some((reader, writer))),
            Err(error) => refused(error),
        }
    }

    /// Where the part file of `item` continues: its length, when the source is unchanged, the
    /// part no longer than it and its tail the source's; else 0, from the start.
    async fn resume_offset(&mut self, item: &Item, part: &Path) -> Result<u64, Step> {
        let existing = match self.dst.stat(part).await {
            Ok(existing) if !existing.is_dir_like() => existing,
            Ok(_) | Err(VfsError::NotFound { .. }) => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let now = self.src.stat(&item.path).await?;
        let mark = FileMark::new(&item.path, &item.entry, part, false);
        if !mark.unchanged(&now) {
            return Ok(0);
        }
        let offset = existing.size;
        if offset == 0 || offset > now.size {
            return Ok(0);
        }
        if self.tail_matches(&item.path, part, offset).await? {
            Ok(offset)
        } else {
            Ok(0)
        }
    }

    /// Whether the [`TAIL_CHECK`] bytes before `offset` are the same in source and part.
    async fn tail_matches(&self, source: &Path, part: &Path, offset: u64) -> Result<bool, Step> {
        let len = offset.min(TAIL_CHECK);
        let from = offset - len;
        let Some(theirs) = read_range(self.src.as_ref(), source, from, len).await? else {
            return Ok(false);
        };
        let Some(ours) = read_range(self.dst.as_ref(), part, from, len).await? else {
            return Ok(false);
        };
        Ok(theirs == ours)
    }

    /// Feeds the source's first `len` bytes to `hasher`, for the check after a continued copy.
    async fn hash_prefix(
        &mut self,
        source: &Path,
        len: u64,
        hasher: &mut blake3::Hasher,
    ) -> Result<(), Step> {
        let mut reader = self.src.open_read(source).await?.take(len);
        let mut buffer = vec![0_u8; self.buffer_size];
        loop {
            self.checkpoint().await?;
            let read = self.job.control.or_cancel(reader.read(&mut buffer)).await?;
            match read.map_err(|error| OpsError::from_io(error, source))? {
                0 => return Ok(()),
                count => {
                    hasher.update(&buffer[..count]);
                }
            }
        }
    }

    /// After a lost connection (`why`): a growing wait and a new backend, or - the retries
    /// spent - a pause until the job is resumed, with `why` among its failures while it waits.
    /// A cancel meanwhile stops the job.
    async fn recover(&mut self, retries: &mut u32, why: Failure) -> Result<(), Step> {
        if *retries >= self.retry.attempts {
            *retries = 0;
            self.job.update(|snapshot| {
                snapshot.resumable = true;
                snapshot.failures.push(why);
            });
            if self.job.control.pause() {
                self.job.emit(JobChange::Paused);
            }
            let resumed = self.checkpoint().await;
            // Nothing else runs while the job is paused: the last failure is `why`.
            self.job.update(|snapshot| {
                snapshot.resumable = false;
                snapshot.failures.pop();
            });
            resumed?;
        } else {
            let wait = self.retry.wait(*retries);
            *retries += 1;
            self.job.control.or_cancel(tokio::time::sleep(wait)).await?;
            self.checkpoint().await?;
        }
        self.reconnect().await;
        Ok(())
    }

    /// The job's server backends as the [`Reconnect`] hook has them now; one that cannot be had
    /// stays, and the next attempt fails and counts.
    async fn reconnect(&mut self) {
        let Some(hook) = self.reconnect.clone() else {
            return;
        };
        let src_id = self.src_id.clone();
        if is_remote(&src_id)
            && let Ok(vfs) = hook.reconnect(&src_id).await
        {
            self.src = vfs;
        }
        match self.dst_id.clone() {
            Some(dst_id) if dst_id == src_id => self.dst = Arc::clone(&self.src),
            Some(dst_id) if is_remote(&dst_id) => {
                if let Ok(vfs) = hook.reconnect(&dst_id).await {
                    self.dst = vfs;
                }
            }
            _ => {}
        }
    }

    /// Tells the store which file the job writes now: a file large enough to be worth resuming
    /// after a restart.
    async fn keep_mark(&self, item: &Item, placement: &Placement) {
        let (Some(store), Some(record)) = (&self.store, self.record) else {
            return;
        };
        if item.entry.size < MARK_MIN_BYTES {
            return;
        }
        let replace = placement.mode == WriteMode::Truncate;
        let mark = FileMark::new(&item.path, &item.entry, &placement.dest, replace);
        store.mark(record, &mark).await;
    }
}

/// `None` for a refusal (the file starts over), the error for a lost connection.
fn refused<T>(error: VfsError) -> Result<Option<T>, Step> {
    let error = OpsError::from(error);
    if is_connection_lost(&error) {
        Err(error.into())
    } else {
        Ok(None)
    }
}

/// `len` bytes of `path` from `from`; `None` when the backend cannot read there or the file is
/// shorter. A lost connection is an error.
async fn read_range(
    vfs: &dyn Vfs,
    path: &Path,
    from: u64,
    len: u64,
) -> Result<Option<Vec<u8>>, Step> {
    let stream = match vfs.open_read_at(path, from).await {
        Ok(stream) => stream,
        Err(error) => return refused(error),
    };
    let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    match stream.take(len).read_to_end(&mut bytes).await {
        Ok(_) => Ok((bytes.len() as u64 == len).then_some(bytes)),
        Err(error) => refused(VfsError::from_io(error, path)),
    }
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
