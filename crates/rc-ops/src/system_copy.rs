//! A copy from disk to disk through the operating system's own call (RC-110-65,
//! `rc_vfs::Vfs::copy_file`): `CopyFileExW` on Windows - a copy within one SMB share stays on the
//! server -, `copy_file_range` on Linux - a reflink where the file system has one. A file of
//! [`SYSTEM_COPY_MIN`] bytes or more takes it between two places on this machine's disks, unless
//! the job verifies (the stream computes the source's checksum on its way) or the file's part is
//! continued; everything else is streamed (`stream_pair.rs`).
//!
//! It keeps what the stream keeps: the progress counts the bytes the system copied, a pause
//! holds the copy at its next piece and a cancel stops it; the file is written under a hidden
//! temporary name ([`system_copy_path`]) and takes its target's name as a part does
//! ([`Runner::settle`], the overwrite rules with it), then gets its zone ([`Runner::keep_zone`],
//! RC-110-56) and its time and attributes ([`Runner::preserve`]). A cancel, a failure or a name
//! refused leaves nothing.
//!
//! A system copy that fails - a file system without the call, a file another program holds open
//! for writing, a share that stopped answering - is not the file's failure: the file is streamed
//! instead, from its start, and the stream's outcome counts, its retries after a lost connection
//! included.

use std::{path::Path, sync::Arc, time::Duration};

use rc_vfs::{BackendId, CopyWatch};
use tokio::time::MissedTickBehavior;

use crate::{
    copy::Moved,
    resume::Placement,
    runner::{Runner, Step},
    staging::remove_leftover,
    temp_name::{part_path, system_copy_path},
    walk::Item,
};

/// The smallest file copied through the system: below it a file is a few of the stream's
/// buffers, and the call's own work - on Windows the file's streams and attributes looked at -
/// costs more than it saves.
pub(crate) const SYSTEM_COPY_MIN: u64 = 1 << 20;

/// How often a running system copy's bytes are counted and the job's pause and cancel looked at.
const LOOK: Duration = Duration::from_millis(50);

impl Runner {
    /// Whether `item` is copied through the system: disk to disk, large enough, a fresh copy
    /// (not a part continued) of a job that does not verify.
    fn by_system(&self, item: &Item, placement: &Placement) -> bool {
        self.src_id == BackendId::Local
            && self.dst_id == Some(BackendId::Local)
            && !self.options.verify
            && !placement.resume
            && item.entry.size >= SYSTEM_COPY_MIN
    }

    /// Copies `item` to `placement.dest` through the system's own copy. `None` where the file
    /// goes the stream's way: it does not qualify, or the system's copy failed - nothing of it is
    /// left then, and the progress stands where it stood.
    pub(crate) async fn copy_by_system(
        &mut self,
        item: &Item,
        placement: &Placement,
    ) -> Result<Option<Moved>, Step> {
        if !self.by_system(item, placement) {
            return Ok(None);
        }
        let temp = system_copy_path(&placement.dest, self.job.id);
        // What a run killed in the middle left there, and a part an earlier copy left that is
        // not continued: the stream would write over it.
        remove_leftover(self.dst.as_ref(), &temp).await;
        remove_leftover(self.dst.as_ref(), &part_path(&placement.dest)).await;
        let start = self.progress.bytes_done;
        let copied = match self.run_system_copy(item, &temp).await {
            Ok(copied) => copied,
            Err(step) => {
                remove_leftover(self.dst.as_ref(), &temp).await;
                return match step {
                    Step::Stop(stop) => Err(stop.into()),
                    Step::Fail(error) => {
                        tracing::debug!(
                            code = error.code(),
                            "the system's copy failed, the file is streamed: {error}"
                        );
                        self.progress.restart_file(start);
                        Ok(None)
                    }
                };
            }
        };
        // Never compared: a job that verifies streams its files.
        let not_verified = blake3::Hasher::new().finalize();
        let landed = match self.check_copy(item, &temp, &not_verified, copied).await {
            Ok(()) => self
                .settle(&temp, &placement.dest, placement.mode)
                .await
                .map_err(|unsettled| (unsettled.step, unsettled.keep_part)),
            Err(step) => Err((step, false)),
        };
        if let Err((step, keep)) = landed {
            if !keep {
                remove_leftover(self.dst.as_ref(), &temp).await;
            }
            return Err(step);
        }
        self.keep_zone(item, &placement.dest).await;
        self.preserve(&item.entry, &placement.dest).await;
        Ok(Some(Moved::Copied))
    }

    /// Runs the system's copy of `item` to `temp` to its end, counting its bytes as progress; a
    /// pause holds it, a cancel stops it - and waits until its thread removed what it wrote. A
    /// failure is the system copy's own: the stream tries the file then.
    async fn run_system_copy(&mut self, item: &Item, temp: &Path) -> Result<u64, Step> {
        let watch = Arc::new(CopyWatch::new());
        let dst = Arc::clone(&self.dst);
        let copy = dst.copy_file(&item.path, temp, Arc::clone(&watch));
        tokio::pin!(copy);
        let mut look = tokio::time::interval(LOOK);
        look.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut counted = 0_u64;
        loop {
            tokio::select! {
                biased;
                done = &mut copy => {
                    let copied = done?;
                    self.count_copied(&mut counted, copied);
                    return Ok(copied);
                }
                _ = look.tick() => {
                    self.count_copied(&mut counted, watch.copied());
                    let control = &self.job.control;
                    if control.is_paused() || control.is_cancelled() {
                        watch.set_paused(true);
                        let held = self.checkpoint().await;
                        watch.set_paused(false);
                        if let Err(stop) = held {
                            watch.cancel();
                            // Its answer is the cancel; what counts is that it ended.
                            let _ = (&mut copy).await;
                            return Err(stop.into());
                        }
                    }
                }
            }
        }
    }

    /// Counts the bytes of `copied` not yet `counted` as progress.
    fn count_copied(&mut self, counted: &mut u64, copied: u64) {
        if copied > *counted {
            self.progress.count(copied - *counted);
            *counted = copied;
            self.tick(false);
        }
    }
}
