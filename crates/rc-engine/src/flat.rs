//! A panel's flat view: files from a whole branch in one list, as Total Commander's branch view
//! (Ctrl+B, Ctrl+Shift+B for the selection) and its "Feed to listbox" of search hits show them.
//!
//! The panel keeps its folder as `path`; every entry is a file from below it (or anywhere, for
//! fed hits outside it), named by its path relative to the folder (`sub\file.txt`), so the
//! commands that join a name to the panel's folder - open, copy, move, delete, the viewer - reach
//! the real file without a mapping. Folders themselves are not listed. The list is filled in the
//! background (`commands/flat.rs`): the panel shows what was found so far, sorted in its order,
//! and stops at [`MAX_FLAT_ENTRIES`]; a panel that shows another directory ends the run.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};

/// The files a flat view lists at most; it stops there and says it was cut.
pub const MAX_FLAT_ENTRIES: usize = 100_000;

/// What a flat view lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlatSource {
    /// Every file below the panel's folder (Ctrl+B).
    Branch,
    /// Every file below the selected folders, and the selected files (Ctrl+Shift+B).
    Selection,
    /// Given paths, such as the hits of a search ("Feed to listbox").
    Paths,
}

/// A panel's flat view as commands report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatView {
    pub source: FlatSource,
    /// Still reading: more entries may arrive.
    pub loading: bool,
    /// Stopped at [`MAX_FLAT_ENTRIES`].
    pub truncated: bool,
    /// Folders and files that could not be read.
    pub skipped: u64,
    /// What the view reads, kept so a refresh reads it again.
    #[serde(skip)]
    pub(crate) plan: Arc<FlatPlan>,
    /// The run that fills the view; a run whose number the panel no longer shows stops.
    #[serde(skip)]
    pub(crate) run: u64,
}

/// What a flat view reads, all paths of the panel's backend.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FlatPlan {
    /// Folders whose files, at every depth, are listed.
    pub(crate) walk: Vec<PathBuf>,
    /// Entries listed as they are: selected files, fed paths.
    pub(crate) stat: Vec<PathBuf>,
}

/// A new run's number.
pub(crate) fn next_run() -> u64 {
    static RUNS: AtomicU64 = AtomicU64::new(1);
    RUNS.fetch_add(1, Ordering::Relaxed)
}

/// The name `path` has in a flat view of `root`: relative to it when it lies below it, else the
/// whole path (a fed hit from another start folder).
pub(crate) fn flat_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "flat_tests.rs"]
mod tests;
