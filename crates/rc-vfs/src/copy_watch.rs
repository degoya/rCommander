//! [`CopyWatch`]: what a copy through the operating system's own call ([`crate::Vfs::copy_file`],
//! RC-110-65) and its caller share while it runs. The copy runs on a blocking thread in one call
//! the caller cannot step through as it steps through a stream; the watch is how it still sees
//! the bytes go and still pauses and cancels the copy - the system's copy looks at it between two
//! of its pieces (`CopyFileExW`'s progress routine, one `copy_file_range` call).

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

/// How often a paused copy looks whether it may go on.
const PAUSED_LOOK: Duration = Duration::from_millis(50);

/// The bytes a system copy has done, and whether it is to wait or to stop. Shared: the caller
/// holds one `Arc`, the copying thread another.
#[derive(Debug, Default)]
pub struct CopyWatch {
    copied: AtomicU64,
    paused: AtomicBool,
    cancelled: AtomicBool,
}

impl CopyWatch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The bytes of the file's contents copied so far.
    #[must_use]
    pub fn copied(&self) -> u64 {
        self.copied.load(Ordering::Relaxed)
    }

    /// Holds the copy at its next piece (`true`) or lets it go on.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Stops the copy at its next piece, a paused one too; what it wrote is removed.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// The copying side: `copied` bytes are done.
    pub fn report(&self, copied: u64) {
        self.copied.store(copied, Ordering::Relaxed);
    }

    /// The copying side, before each piece: waits while the copy is paused; whether it goes on
    /// (`false` once cancelled). Blocking.
    #[must_use]
    pub fn go_on(&self) -> bool {
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.is_paused() {
                return true;
            }
            std::thread::sleep(PAUSED_LOOK);
        }
    }
}

/// Cancels its watch when dropped before [`StopOnDrop::done`]: a caller that lets go of the copy
/// - its future dropped, a time limit - stops the copying thread, which removes what it wrote.
pub(crate) struct StopOnDrop(Option<Arc<CopyWatch>>);

impl StopOnDrop {
    pub(crate) fn new(watch: &Arc<CopyWatch>) -> Self {
        Self(Some(Arc::clone(watch)))
    }

    /// The copy ended on its own: nothing to stop.
    pub(crate) fn done(mut self) {
        self.0 = None;
    }
}

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Some(watch) = self.0.take() {
            watch.cancel();
        }
    }
}

#[cfg(test)]
#[path = "copy_watch_tests.rs"]
mod tests;
