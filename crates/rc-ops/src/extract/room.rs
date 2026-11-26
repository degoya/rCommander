//! The room an unpacking needs (RC-110-62): before the first file is written, the files the plans
//! will write are held together against the free space of the target's drive, so an archive that
//! unpacks to more than the drive holds fails at once with `not_enough_space` - the bytes it needs
//! and the bytes free - instead of filling the drive. A backend that cannot tell its free space
//! (a server) is not asked; the archive limits (64 GiB unpacked, the ratio) still bound what it
//! unpacks.

use std::path::Path;

use rc_vfs::Vfs;

use super::Plan;
use crate::{OpsError, runner::Step};

/// The bytes the files of `plans` unpack to, the refused ones left out.
pub(super) fn needed(plans: &[Plan]) -> u64 {
    plans
        .iter()
        .flat_map(|plan| &plan.entries)
        .filter(|planned| planned.refused.is_none() && planned.item.is_file())
        .fold(0, |sum, planned| {
            sum.saturating_add(planned.item.entry.size)
        })
}

/// Refuses `needed` bytes below `target` where its drive has fewer free.
pub(super) async fn check(dst: &dyn Vfs, target: &Path, needed: u64) -> Result<(), Step> {
    match dst.free_space(target).await? {
        Some(free) if free < needed => Err(OpsError::NotEnoughSpace {
            path: target.to_path_buf(),
            needed,
            free,
        }
        .into()),
        _ => Ok(()),
    }
}
