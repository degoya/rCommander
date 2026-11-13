//! What packing and the archive update share around their one blocking task (RC-110-30): its
//! progress followed while it runs, and the unfinished archive removed when it failed.

use std::path::Path;

use tokio::{sync::watch, task::JoinError};

use super::write::Feed;
use crate::{recover::LogPanic as _, runner::Runner};

impl Runner {
    /// Runs `work` with `feed` as a blocking task and follows it until it ends: the bytes the
    /// feed reports through `seen` are the job's progress, the member it reads the current file.
    /// A panic of the task is logged under `task` and comes back as the join error.
    pub(crate) async fn follow_blocking<T, W>(
        &mut self,
        task: &'static str,
        feed: Feed,
        mut seen: watch::Receiver<u64>,
        work: W,
    ) -> Result<T, JoinError>
    where
        T: Send + 'static,
        W: FnOnce(&Feed) -> T + Send + 'static,
    {
        let current = feed.current();
        let start = self.progress.bytes_done;
        let mut running = tokio::task::spawn_blocking(move || work(&feed));
        let joined = loop {
            tokio::select! {
                joined = &mut running => break joined,
                changed = seen.changed() => {
                    if changed.is_err() {
                        break (&mut running).await;
                    }
                    let read = *seen.borrow_and_update();
                    self.progress.bytes_done = start + read;
                    if let Some(member) = current.get() {
                        let at = self.src_path(&member.path);
                        self.progress.begin_file(at, member.size);
                        self.progress.file_bytes_done = read.saturating_sub(member.from);
                    }
                    self.tick(false);
                }
            }
        };
        joined.log_panic(task)
    }
}

/// Removes an unfinished new archive; one that cannot be removed is logged.
pub(crate) async fn remove_temp(temp: &Path) {
    if let Err(left) = tokio::fs::remove_file(temp).await
        && left.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            path = %temp.display(),
            "an unfinished archive could not be removed: {left}"
        );
    }
}
