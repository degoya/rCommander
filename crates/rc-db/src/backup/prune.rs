//! How many automatic backups stay (RC-080-42, the owner's decision of 2026-10-28): after a new
//! backup into the backup folder, the oldest under a generated name beyond the newest `keep` are
//! removed. A backup a person named - any other name in the folder, a file elsewhere - is never
//! removed, and neither is a restore's safety backup.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{BackupError, NAME_EXTENSION, NAME_PREFIX};

/// The fewest and the most backups the setting `backup.keep` may keep.
pub const KEEP_RANGE: std::ops::RangeInclusive<usize> = 1..=1000;

/// The order of a generated name `rcommander-YYYYMMDD-HHMMSS[-N].db`: its stamp and number;
/// `None` for any other name.
fn generated(name: &str) -> Option<(String, u32)> {
    let rest = name
        .strip_prefix(NAME_PREFIX)?
        .strip_suffix(NAME_EXTENSION)?;
    let (stamp, number) = match rest.get(15..) {
        Some("") => (rest, 1),
        Some(tail) => (rest.get(..15)?, tail.strip_prefix('-')?.parse().ok()?),
        None => return None,
    };
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    let (date, time) = stamp.split_once('-')?;
    (digits(date, 8) && digits(time, 6)).then(|| (stamp.to_owned(), number))
}

/// Removes the oldest backups under a generated name in `dir` beyond the newest `keep` (held to
/// [`KEEP_RANGE`]); the files removed, oldest first.
///
/// # Errors
///
/// [`BackupError::Target`] when the folder cannot be listed or a file not removed; what was
/// removed before stays removed.
pub fn prune_backups(dir: &Path, keep: usize) -> Result<Vec<PathBuf>, BackupError> {
    let failed = |path: &Path| {
        let path = path.to_path_buf();
        move |source| BackupError::Target { path, source }
    };
    let keep = keep.clamp(*KEEP_RANGE.start(), *KEEP_RANGE.end());
    let mut found: Vec<((String, u32), PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir).map_err(failed(dir))? {
        let entry = entry.map_err(failed(dir))?;
        // Files only, links not followed: a link under a backup's name is no backup.
        if !entry.file_type().map_err(failed(dir))?.is_file() {
            continue;
        }
        if let Some(order) = entry.file_name().to_str().and_then(generated) {
            found.push((order, entry.path()));
        }
    }
    found.sort();
    let surplus = found.len().saturating_sub(keep);
    let mut removed = Vec::with_capacity(surplus);
    for (_, path) in found.into_iter().take(surplus) {
        fs::remove_file(&path).map_err(failed(&path))?;
        removed.push(path);
    }
    Ok(removed)
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
