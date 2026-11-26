//! A local listing sent in parts (RC-110-66), as SFTP sends one per `READDIR` answer
//! (RC-060-02): a folder of 100 000 entries - on a share or `\\wsl$` far slower than on a disk -
//! shows its first ones while the rest is still being read.
//!
//! The folder is read on one blocking thread, as [`super::LocalFs::list`] reads it; every
//! [`BATCH_ENTRIES`] entries, or when [`BATCH_AGE`] has passed since the last part, the entries
//! read so far go to the receiver. A share that delivers slowly so still shows something within
//! the age, a disk is not cut into parts smaller than the batch. Once the receiver is gone - the
//! panel shows another folder - the reading stops before the next entry, without an error.

use std::{
    mem,
    ops::ControlFlow,
    path::Path,
    time::{Duration, Instant},
};

use tokio::sync::mpsc::Sender;

use super::{WORK, describe};
use crate::{Entry, Result, work::blocking};

/// The entries of one part.
pub(super) const BATCH_ENTRIES: usize = 4096;
/// How long a part waits for more entries before it is sent with fewer.
pub(super) const BATCH_AGE: Duration = Duration::from_millis(100);

/// The entries of `dir` sent to `batches` in parts as they are read.
pub(super) async fn list_batches(dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
    let shown = dir.to_path_buf();
    blocking(WORK, dir, move || {
        send_in_parts(&shown, &batches, BATCH_ENTRIES, BATCH_AGE)?;
        Ok(())
    })
    .await
}

/// Reads `dir` on the calling thread, which may block, and sends its entries to `batches` in
/// parts of `size` entries, or fewer once `age` has passed since the last part. The number of
/// entries read: fewer than the folder holds when the receiver went away.
fn send_in_parts(
    dir: &Path,
    batches: &Sender<Vec<Entry>>,
    size: usize,
    age: Duration,
) -> Result<usize> {
    let (mut part, mut read, mut gone) = (Vec::new(), 0, false);
    let mut since = Instant::now();
    describe::list_each(dir, |entry| {
        if batches.is_closed() {
            gone = true;
            return ControlFlow::Break(());
        }
        read += 1;
        part.push(entry);
        if part.len() >= size || since.elapsed() >= age {
            // The next part as large as this one: a disk fills it without growing it step by
            // step, a slow share's few entries hold no room for a whole part (RC-110-93).
            let next = Vec::with_capacity(part.len());
            if batches
                .blocking_send(mem::replace(&mut part, next))
                .is_err()
            {
                gone = true;
                return ControlFlow::Break(());
            }
            since = Instant::now();
        }
        ControlFlow::Continue(())
    })?;
    if !gone && !part.is_empty() {
        // A receiver that is gone wants nothing more.
        let _ = batches.blocking_send(part);
    }
    Ok(read)
}

#[cfg(test)]
#[path = "batches_tests.rs"]
mod tests;
