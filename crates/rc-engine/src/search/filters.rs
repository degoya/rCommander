//! The size, date, attribute and kind filters of a search.

use std::time::{Duration, SystemTime};

use rc_vfs::{Entry, EntryKind, TargetKind, unix_time};
use serde::{Deserialize, Serialize};

/// Which kinds of entries can be hits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindFilter {
    /// Files and directories.
    #[default]
    Any,
    /// Files only (and links to files).
    Files,
    /// Directories only (and links to directories).
    Dirs,
}

/// Conditions an entry has to meet besides its name; every one that is set has to hold (the
/// intersection). A size or content condition is only met by files, as in Total Commander.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub kind: KindFilter,
    /// At least this many bytes.
    pub min_size: Option<u64>,
    /// At most this many bytes.
    pub max_size: Option<u64>,
    /// Modified at or after this time (Unix seconds).
    pub modified_after: Option<u64>,
    /// Modified at or before this time (Unix seconds).
    pub modified_before: Option<u64>,
    /// Modified at most this many seconds ago ("not older than").
    pub max_age_secs: Option<u64>,
    /// `Some(true)`: only hidden entries; `Some(false)`: only visible ones.
    pub hidden: Option<bool>,
    /// Read-only attribute (Windows) or no write bit (Unix).
    pub readonly: Option<bool>,
    /// Windows system attribute; off Windows nothing has it.
    pub system: Option<bool>,
    /// Windows archive attribute; off Windows nothing has it.
    pub archive: Option<bool>,
    /// Unix permission bits that all have to be set (`0o111`: executable by everyone).
    pub mode_bits: Option<u32>,
}

impl Filters {
    /// Whether a size condition is set.
    pub(crate) fn wants_size(&self) -> bool {
        self.min_size.is_some() || self.max_size.is_some()
    }

    /// Whether `entry` meets every condition, measured against `now`.
    pub(crate) fn admits(&self, entry: &Entry, now: SystemTime) -> bool {
        let (file, dir) = shape(entry);
        let kind = match self.kind {
            KindFilter::Any => true,
            KindFilter::Files => file,
            KindFilter::Dirs => dir,
        };
        kind && self.size_fits(entry, file)
            && self.date_fits(entry, now)
            && self.attributes_fit(entry)
    }

    fn size_fits(&self, entry: &Entry, file: bool) -> bool {
        if !self.wants_size() {
            return true;
        }
        file && self.min_size.is_none_or(|min| entry.size >= min)
            && self.max_size.is_none_or(|max| entry.size <= max)
    }

    fn date_fits(&self, entry: &Entry, now: SystemTime) -> bool {
        // A bound past what the clock can hold (`Some(None)`): nothing was modified after it,
        // everything with a time before it.
        let after = match self.modified_after.map(unix_time) {
            Some(None) => return false,
            bound => bound.flatten(),
        };
        let after = self
            .max_age_secs
            .and_then(|age| now.checked_sub(Duration::from_secs(age)))
            .into_iter()
            .chain(after)
            .max();
        let before = self.modified_before.map(unix_time);
        if after.is_none() && before.is_none() {
            return true;
        }
        entry.modified.is_some_and(|modified| {
            after.is_none_or(|after| modified >= after)
                && before.flatten().is_none_or(|before| modified <= before)
        })
    }

    fn attributes_fit(&self, entry: &Entry) -> bool {
        let windows = entry.attributes.windows.unwrap_or_default();
        let wanted = |want: Option<bool>, has: bool| want.is_none_or(|want| want == has);
        wanted(self.hidden, entry.hidden)
            && wanted(self.readonly, entry.attributes.readonly)
            && wanted(self.system, windows.system)
            && wanted(self.archive, windows.archive)
            && self.mode_bits.is_none_or(|bits| {
                entry
                    .attributes
                    .mode
                    .is_some_and(|mode| mode & bits == bits)
            })
    }
}

/// Whether an entry counts as a file and as a directory; a link counts as its target.
pub(crate) fn shape(entry: &Entry) -> (bool, bool) {
    match &entry.kind {
        EntryKind::File => (true, false),
        EntryKind::Dir => (false, true),
        EntryKind::Symlink { target_kind, .. } => (
            *target_kind == Some(TargetKind::File),
            *target_kind == Some(TargetKind::Dir),
        ),
        EntryKind::Other => (false, false),
    }
}

#[cfg(test)]
#[path = "filters_tests.rs"]
mod tests;
