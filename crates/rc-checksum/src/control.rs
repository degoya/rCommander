//! Cancellation, pause and progress of one run, and the cleanup of what a run that does not
//! finish has written.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rc_vfs::Vfs;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::ChecksumError;

/// Why the file at a path must not be read in a run, or `None` when it may
/// ([`Control::with_guard`]).
pub type ReadGuard = Arc<dyn Fn(&Path) -> Option<&'static str> + Send + Sync>;

/// The handle of one run: cancel or pause it from anywhere, read how far it has got. Clones
/// share all of it.
#[derive(Debug, Clone, Default)]
pub struct Control {
    cancel: CancellationToken,
    done: Arc<AtomicU64>,
    counts: Arc<Counts>,
    pause: Arc<Pause>,
    guard: Guard,
}

/// What a run plans and has finished, as far as it knows them; for a job's progress.
#[derive(Debug, Default)]
struct Counts {
    bytes_total: AtomicU64,
    files_total: AtomicU64,
    files_done: AtomicU64,
    /// The file at work, its size and its bytes read so far (RC-010-07).
    file: Mutex<Option<PathBuf>>,
    file_bytes_total: AtomicU64,
    file_bytes_done: AtomicU64,
}

/// Whether the run is paused; it waits at its next buffer while it is.
#[derive(Debug)]
struct Pause(watch::Sender<bool>);

impl Default for Pause {
    fn default() -> Self {
        Self(watch::Sender::new(false))
    }
}

/// The optional [`ReadGuard`] of a run; `Debug` without the closure.
#[derive(Clone, Default)]
struct Guard(Option<ReadGuard>);

impl fmt::Debug for Guard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() {
            "Guard(..)"
        } else {
            "None"
        })
    }
}

impl Control {
    /// A run that is not cancelled and has read nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A run that stops when `cancel` is cancelled (a job queue's token).
    #[cfg(test)]
    #[must_use]
    pub fn with_token(cancel: CancellationToken) -> Self {
        Self {
            cancel,
            ..Self::default()
        }
    }

    /// The run reads no file `guard` names a reason for: [`crate::hash_file`] fails on it with
    /// `sensitive_path`, so a folder's checksums or a checksum file's lines never carry the
    /// digest of such a file. The MCP server's runs have one; the window's have none.
    #[must_use]
    pub fn with_guard(mut self, guard: ReadGuard) -> Self {
        self.guard = Guard(Some(guard));
        self
    }

    /// Stops the run at its next buffer; what it wrote is removed.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Whether [`Control::cancel`] was called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Holds the run at its next buffer until [`Control::resume`]; a cancel still stops it.
    pub fn pause(&self) {
        self.pause.0.send_replace(true);
    }

    /// Lets a paused run go on.
    pub fn resume(&self) {
        self.pause.0.send_replace(false);
    }

    /// Whether [`Control::pause`] holds the run.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        *self.pause.0.borrow()
    }

    /// Bytes read so far, over every file of the run.
    #[must_use]
    pub fn bytes_done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }

    /// The bytes the run will read, once it knows them; 0 before, and for a check of a
    /// checksum file, which only counts its lines.
    #[must_use]
    pub fn bytes_total(&self) -> u64 {
        self.counts.bytes_total.load(Ordering::Relaxed)
    }

    /// The files (parts, lines) the run will handle, once it knows them.
    #[must_use]
    pub fn files_total(&self) -> u64 {
        self.counts.files_total.load(Ordering::Relaxed)
    }

    /// The files (parts, lines) handled so far.
    #[must_use]
    pub fn files_done(&self) -> u64 {
        self.counts.files_done.load(Ordering::Relaxed)
    }

    /// The file the run reads now (the part of a combine, the source of a split); `None`
    /// before the first.
    #[must_use]
    pub fn current_file(&self) -> Option<PathBuf> {
        self.counts
            .file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The bytes of [`Control::current_file`] read so far; 0 again with each file.
    #[must_use]
    pub fn file_bytes_done(&self) -> u64 {
        self.counts.file_bytes_done.load(Ordering::Relaxed)
    }

    /// The size of [`Control::current_file`] as counted before it was opened.
    #[must_use]
    pub fn file_bytes_total(&self) -> u64 {
        self.counts.file_bytes_total.load(Ordering::Relaxed)
    }

    pub(crate) fn check(&self) -> Result<(), ChecksumError> {
        if self.is_cancelled() {
            Err(ChecksumError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// [`Control::check`], first waiting while the run is paused.
    pub(crate) async fn checkpoint(&self) -> Result<(), ChecksumError> {
        self.check()?;
        if self.is_paused() {
            let mut paused = self.pause.0.subscribe();
            tokio::select! {
                () = self.cancel.cancelled() => {}
                // The sender lives as long as `self`: `wait_for` cannot fail while it waits.
                _ = paused.wait_for(|paused| !paused) => {}
            }
        }
        self.check()
    }

    /// Adds `files` files (parts, lines) of `bytes` bytes to what the run will handle.
    pub(crate) fn plan(&self, files: u64, bytes: u64) {
        self.counts.files_total.fetch_add(files, Ordering::Relaxed);
        self.counts.bytes_total.fetch_add(bytes, Ordering::Relaxed);
    }

    /// One file (part, line) is handled.
    pub(crate) fn file_done(&self) {
        self.counts.files_done.fetch_add(1, Ordering::Relaxed);
    }

    /// The file at `path` of `size` bytes is read next: its own bytes start at 0.
    pub(crate) fn begin_file(&self, path: &Path, size: u64) {
        let mut file = self
            .counts
            .file
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.counts.file_bytes_done.store(0, Ordering::Relaxed);
        self.counts.file_bytes_total.store(size, Ordering::Relaxed);
        *file = Some(path.to_path_buf());
    }

    /// Refuses `path` when the run's guard names a reason. The guard may follow links and
    /// probe the file - blocking calls, on a network drive perhaps - so it runs on the blocking
    /// pool, not on a thread of the runtime (RC-080-42).
    pub(crate) async fn admit(&self, path: &Path) -> Result<(), ChecksumError> {
        let Some(guard) = self.guard.0.clone() else {
            return Ok(());
        };
        let owned = path.to_path_buf();
        let judged = tokio::task::spawn_blocking(move || guard(&owned)).await;
        let reason = match judged {
            Ok(reason) => reason,
            // The blocking pool is shutting down: judged here, as it always was.
            Err(_) => self.guard.0.as_ref().and_then(|guard| guard(path)),
        };
        match reason {
            Some(reason) => Err(ChecksumError::Sensitive {
                path: path.to_path_buf(),
                reason,
            }),
            None => Ok(()),
        }
    }

    /// `bytes` more read, of the run and of the file at work.
    pub(crate) fn advance(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
        self.counts
            .file_bytes_done
            .fetch_add(bytes, Ordering::Relaxed);
    }
}

/// The files a run has created and not yet handed over. Unless [`Pending::keep`] is called they
/// are removed: by [`Pending::discard`] after a failure or a cancel, and - when the run's future
/// is dropped half way - by a task the drop spawns, so no half file stays behind either way.
pub(crate) struct Pending {
    vfs: Arc<dyn Vfs>,
    paths: Vec<PathBuf>,
}

impl Pending {
    pub(crate) fn new(vfs: &Arc<dyn Vfs>) -> Self {
        Self {
            vfs: Arc::clone(vfs),
            paths: Vec::new(),
        }
    }

    /// Records a file the run is about to create.
    pub(crate) fn push(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    /// The run finished: every file stays.
    pub(crate) fn keep(mut self) {
        self.paths.clear();
    }

    /// The run failed: every file it created goes, now.
    pub(crate) async fn discard(mut self) {
        for path in std::mem::take(&mut self.paths) {
            remove_quietly(&self.vfs, path).await;
        }
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if self.paths.is_empty() {
            return;
        }
        let paths = std::mem::take(&mut self.paths);
        let vfs = Arc::clone(&self.vfs);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    for path in paths {
                        remove_quietly(&vfs, path).await;
                    }
                });
            }
            Err(_) => tracing::warn!(
                files = paths.len(),
                "an unfinished checksum run left files behind: no runtime to remove them"
            ),
        }
    }
}

async fn remove_quietly(vfs: &Arc<dyn Vfs>, path: PathBuf) {
    if let Err(error) = vfs.remove_file(&path).await
        && error.code() != "not_found"
    {
        tracing::warn!(
            code = error.code(),
            "an unfinished file could not be removed"
        );
    }
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
