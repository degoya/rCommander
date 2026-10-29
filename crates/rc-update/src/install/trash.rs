//! What an update could not remove yet (T-TRASH): leftovers are moved into `<install>/.trash/`
//! and removed from there; whatever the system still holds stays for a later sweep instead of
//! failing the update. Windows lets a running executable be moved but not deleted: an
//! `rcommander mcp` a client started from the old file keeps running from `.previous/`, which
//! therefore cannot be removed until it ends - and must not stop the next update.
//!
//! Copied from rDownloader's `crates/rd-update/src/install/trash.rs` at commit 26c8592a6
//! (RC-100-06), without the crash point.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;

use super::durable::{exists, remove_any, rename};

/// Beside the program files: the leftovers of earlier updates that could not be removed yet.
pub const TRASH_DIR: &str = ".trash";

/// `<install>/.trash`.
#[must_use]
pub fn trash_dir(install: &Path) -> PathBuf {
    install.join(TRASH_DIR)
}

/// Moves `leftover`, a folder or file inside `install`, into the trash, then removes from the
/// trash what can go now ([`sweep`]). Nothing happens when it does not exist.
///
/// # Errors
///
/// When the leftover can be moved neither whole nor entry by entry.
pub fn discard(leftover: &Path, install: &Path) -> Result<()> {
    if !exists(leftover) {
        return Ok(());
    }
    let name = leftover
        .file_name()
        .with_context(|| format!("{} has no name", leftover.display()))?;
    let batch = new_batch(install)?;
    set_aside(leftover, &batch.join(name))?;
    sweep(install);
    Ok(())
}

/// Removes everything in the trash of `install` the system lets go; the rest stays for the next
/// sweep, and the trash goes once it is empty. Never fails: what stays is logged.
pub fn sweep(install: &Path) {
    let trash = trash_dir(install);
    let entries = match fs::read_dir(&trash) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, trash = %trash.display(), "the update trash cannot be read");
            return;
        }
    };
    let mut emptied = true;
    for entry in entries.flatten() {
        if let Err(error) = remove_any(&entry.path()) {
            emptied = false;
            tracing::warn!(
                error = format!("{error:#}"),
                "an update leftover is still in use and stays in the trash until a later start"
            );
        }
    }
    if emptied {
        let _ = fs::remove_dir(&trash);
    }
}

/// A new, empty folder in the trash, named by the time, for one leftover.
fn new_batch(install: &Path) -> Result<PathBuf> {
    let trash = trash_dir(install);
    fs::create_dir_all(&trash).with_context(|| format!("create {}", trash.display()))?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
    for attempt in 0..100_u32 {
        let batch = trash.join(format!("{stamp}-{attempt}"));
        match fs::create_dir(&batch) {
            Ok(()) => return Ok(batch),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| format!("create {}", batch.display()));
            }
        }
    }
    bail!("no free name in {}", trash.display())
}

/// Moves `from` to `to`: whole when the system lets it, otherwise - a folder holding a running
/// program on Windows - every entry on its own, then the emptied folder is removed.
fn set_aside(from: &Path, to: &Path) -> Result<()> {
    let Err(refused) = fs::rename(from, to) else {
        return Ok(());
    };
    let is_folder = fs::symlink_metadata(from).is_ok_and(|metadata| metadata.is_dir());
    if !is_folder {
        return rename(from, to).with_context(|| format!("first attempt: {refused}"));
    }
    fs::create_dir_all(to).with_context(|| format!("create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("list {}", from.display()))? {
        let entry = entry.with_context(|| format!("list {}", from.display()))?;
        set_aside(&entry.path(), &to.join(entry.file_name()))?;
    }
    fs::remove_dir(from).with_context(|| format!("remove {}", from.display()))
}

#[cfg(test)]
#[path = "trash_tests.rs"]
mod tests;
