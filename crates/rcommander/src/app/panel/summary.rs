//! The status line's numbers, as Total Commander shows them under a panel: the bytes, files and
//! directories selected against those of the whole directory.

use std::collections::BTreeMap;

use rc_engine::{DirSize, Entry, PanelState};

/// The most names a selection change may change for [`Summary::reselected`]; beyond it the
/// entries are counted again.
const MAX_CHANGED: usize = 16;

/// Counts and sums of a panel. Directories count apart and add their bytes only once counted
/// (Space, Alt+Shift+Enter: RC-020-06).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub files: usize,
    pub dirs: usize,
    pub bytes: u64,
    pub selected_files: usize,
    pub selected_dirs: usize,
    pub selected_bytes: u64,
}

impl Summary {
    /// The numbers of `state`: one pass over the entries.
    pub fn of(state: &PanelState) -> Self {
        let mut summary = Self::default();
        for entry in state.entries.iter() {
            let selected = state.selection.contains(&entry.name);
            if entry.is_dir_like() {
                summary.dirs += 1;
                summary.selected_dirs += usize::from(selected);
                if let Some(DirSize::Bytes(bytes)) = state.dir_sizes.get(&entry.name) {
                    summary.bytes += bytes;
                    if selected {
                        summary.selected_bytes += bytes;
                    }
                }
            } else {
                summary.files += 1;
                summary.bytes += entry.size;
                if selected {
                    summary.selected_files += 1;
                    summary.selected_bytes += entry.size;
                }
            }
        }
        summary
    }

    /// The numbers after only the selection changed from `old` to `new` - the same entries, the
    /// same counted sizes: only the names that changed are looked at, the entry under either
    /// cursor first (Insert, Space), so a toggle costs no pass over the folder (RC-080-53).
    /// `None` when more changed, or a name is not listed; [`Summary::of`] counts again.
    pub fn reselected(&self, old: &PanelState, new: &PanelState) -> Option<Self> {
        let changed: Vec<&String> = old
            .selection
            .symmetric_difference(&new.selection)
            .take(MAX_CHANGED + 1)
            .collect();
        if changed.len() > MAX_CHANGED {
            return None;
        }
        let mut summary = *self;
        for name in changed {
            let entry = [old.cursor, new.cursor]
                .into_iter()
                .filter_map(|at| new.entries.get(at))
                .find(|entry| entry.name == *name)
                .or_else(|| new.entries.iter().find(|entry| entry.name == *name))?;
            summary.mark(entry, new.selection.contains(name), &new.dir_sizes);
        }
        Some(summary)
    }

    /// `entry` selected (`on`) or no longer.
    fn mark(&mut self, entry: &Entry, on: bool, dir_sizes: &BTreeMap<String, DirSize>) {
        let (count, bytes) = if entry.is_dir_like() {
            let bytes = match dir_sizes.get(&entry.name) {
                Some(DirSize::Bytes(bytes)) => *bytes,
                _ => 0,
            };
            (&mut self.selected_dirs, bytes)
        } else {
            (&mut self.selected_files, entry.size)
        };
        if on {
            *count += 1;
            self.selected_bytes += bytes;
        } else {
            *count = count.saturating_sub(1);
            self.selected_bytes = self.selected_bytes.saturating_sub(bytes);
        }
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
