//! Synchronize dirs (Total Commander's "Synchronize dirs"): compare two trees and propose, per
//! entry, what would make them alike.
//!
//! [`plan`] walks both trees over their [`rc_vfs::Vfs`] and answers a [`SyncPlan`]: one
//! [`SyncRow`] per name with both sides, how they compare ([`PairState`]) and the proposed
//! [`SyncAction`] for the [`SyncMode`]. A person changes actions row by row
//! ([`SyncPlan::set_action`]); [`SyncPlan::operations`] turns the plan into copies, directories
//! and deletes, [`SyncPlan::jobs`] into the `rc-ops` jobs that run them - deletes only once
//! they are confirmed.

mod apply;
mod mask;
mod plan;
mod run;

use std::{path::PathBuf, time::SystemTime};

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use crate::EngineError;
pub use apply::SyncOperation;
pub use mask::Masks;
pub use plan::plan;
pub(crate) use plan::plan_with_lone_dirs;
pub use run::{RowAction, SyncApplyRequest, SyncRun};

/// FAT keeps modification times in steps of two seconds: the default tolerance.
pub const FAT_TOLERANCE_SECS: u32 = 2;

/// Rows a `compare.dirs` answer carries when the request names no limit.
pub const DIR_COMPARE_LIMIT: usize = 1000;

/// Which way a synchronization goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// Both ways: what is missing or older on one side is copied from the other.
    #[default]
    Both,
    /// Only left to right: missing or older files on the right are copied from the left.
    LeftToRight,
    /// Only right to left.
    RightToLeft,
    /// Asymmetric: the right becomes like the left - including deleting what only the right
    /// has, which needs a confirmation.
    MirrorToRight,
    /// Asymmetric the other way: the left becomes like the right.
    MirrorToLeft,
}

/// How two trees are compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncOptions {
    pub mode: SyncMode,
    /// Descend into subdirectories; without, only the files of the two directories.
    pub subdirs: bool,
    /// Compare files of the same size byte by byte; identical ones are equal whatever their
    /// times say.
    pub by_content: bool,
    /// Compare by size (and content) only, not by modification time.
    pub ignore_date: bool,
    /// Times this many seconds apart count as the same ([`FAT_TOLERANCE_SECS`] by default).
    pub time_tolerance_secs: u32,
    /// Times exactly one hour apart (within the tolerance) count as the same: a daylight
    /// saving shift between file systems that store local time and those that store UTC.
    pub ignore_dst_hour: bool,
    /// Which names take part ([`Masks`]); everything when empty.
    pub masks: String,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            mode: SyncMode::default(),
            subdirs: true,
            by_content: false,
            ignore_date: false,
            time_tolerance_secs: FAT_TOLERANCE_SECS,
            ignore_dst_hour: false,
            masks: String::new(),
        }
    }
}

/// File or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    File,
    Dir,
}

/// One side of a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncItem {
    pub kind: ItemKind,
    /// Bytes of a file; 0 for a directory.
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// How the two sides of a row compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairState {
    /// `=`: the same (by the options), or a directory on both sides.
    Equal,
    /// Only the left has it.
    LeftOnly,
    /// Only the right has it.
    RightOnly,
    /// The left one is newer.
    LeftNewer,
    /// The right one is newer.
    RightNewer,
    /// `≠`: different, and the times do not say which is newer.
    Different,
    /// A file on one side, a directory on the other.
    KindMismatch,
}

/// What a row asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    /// Leave both sides as they are.
    #[default]
    None,
    /// Copy the left one to the right.
    CopyToRight,
    /// Copy the right one to the left.
    CopyToLeft,
    /// Delete the right one.
    DeleteRight,
    /// Delete the left one.
    DeleteLeft,
}

/// One name, both sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRow {
    /// The path below both roots.
    pub path: PathBuf,
    pub left: Option<SyncItem>,
    pub right: Option<SyncItem>,
    pub state: PairState,
    pub action: SyncAction,
    /// A directory row with something left out below it (by the masks, a link, a device, or
    /// no subdirectories): it is never copied or deleted as a whole.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

impl SyncRow {
    /// The kind both sides share, or the one side's; `None` for a file against a directory.
    #[must_use]
    pub fn kind(&self) -> Option<ItemKind> {
        match (&self.left, &self.right) {
            (Some(left), Some(right)) if left.kind != right.kind => None,
            (Some(item), _) | (None, Some(item)) => Some(item.kind),
            (None, None) => None,
        }
    }

    /// Whether `action` makes sense here: a copy needs its source, a delete its target, and
    /// neither goes between a file and a directory.
    #[must_use]
    pub fn allows(&self, action: SyncAction) -> bool {
        let mismatch = self.state == PairState::KindMismatch;
        match action {
            SyncAction::None => true,
            SyncAction::CopyToRight => self.left.is_some() && !mismatch,
            SyncAction::CopyToLeft => self.right.is_some() && !mismatch,
            SyncAction::DeleteRight => self.right.is_some(),
            SyncAction::DeleteLeft => self.left.is_some(),
        }
    }
}

/// The action `mode` proposes for a row in `state`. Only the mirror modes overwrite a newer
/// file, delete, or settle a `≠`; the others leave that to the person.
#[must_use]
pub fn proposed_action(state: PairState, mode: SyncMode) -> SyncAction {
    use PairState as S;
    use SyncAction as A;
    match (mode, state) {
        (_, S::Equal | S::KindMismatch) => A::None,
        (SyncMode::Both | SyncMode::LeftToRight, S::LeftOnly | S::LeftNewer)
        | (SyncMode::MirrorToRight, S::LeftOnly | S::LeftNewer | S::RightNewer | S::Different) => {
            A::CopyToRight
        }
        (SyncMode::Both | SyncMode::RightToLeft, S::RightOnly | S::RightNewer)
        | (SyncMode::MirrorToLeft, S::RightOnly | S::RightNewer | S::LeftNewer | S::Different) => {
            A::CopyToLeft
        }
        (SyncMode::MirrorToRight, S::RightOnly) => A::DeleteRight,
        (SyncMode::MirrorToLeft, S::LeftOnly) => A::DeleteLeft,
        _ => A::None,
    }
}

/// Which rows to show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowFilter {
    /// Every row that is not equal.
    #[default]
    Differences,
    All,
    Equal,
    LeftOnly,
    RightOnly,
}

impl RowFilter {
    /// Whether `row` passes.
    #[must_use]
    pub fn passes(self, row: &SyncRow) -> bool {
        match self {
            Self::Differences => row.state != PairState::Equal,
            Self::All => true,
            Self::Equal => row.state == PairState::Equal,
            Self::LeftOnly => row.state == PairState::LeftOnly,
            Self::RightOnly => row.state == PairState::RightOnly,
        }
    }
}

/// Files and bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub files: u64,
    pub bytes: u64,
}

/// The plan in numbers: rows by state, file copies and deletes by direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSummary {
    pub equal: u64,
    pub left_only: u64,
    pub right_only: u64,
    pub left_newer: u64,
    pub right_newer: u64,
    pub different: u64,
    pub kind_mismatch: u64,
    pub copy_to_right: Volume,
    pub copy_to_left: Volume,
    /// Rows to delete on the right, directories included.
    pub delete_right: u64,
    pub delete_left: u64,
}

/// Two trees compared, with an action per row. Rows are in tree order: a directory's row comes
/// right before the rows below it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPlan {
    pub left: VfsPath,
    pub right: VfsPath,
    pub options: SyncOptions,
    pub rows: Vec<SyncRow>,
    /// Entries not compared: links to directories, dangling links, devices and the like.
    pub skipped: u64,
}

impl SyncPlan {
    /// Counts the rows.
    #[must_use]
    pub fn summary(&self) -> SyncSummary {
        let mut summary = SyncSummary::default();
        for row in &self.rows {
            let count = match row.state {
                PairState::Equal => &mut summary.equal,
                PairState::LeftOnly => &mut summary.left_only,
                PairState::RightOnly => &mut summary.right_only,
                PairState::LeftNewer => &mut summary.left_newer,
                PairState::RightNewer => &mut summary.right_newer,
                PairState::Different => &mut summary.different,
                PairState::KindMismatch => &mut summary.kind_mismatch,
            };
            *count += 1;
            let file = |item: &Option<SyncItem>| {
                item.as_ref()
                    .filter(|item| item.kind == ItemKind::File)
                    .map(|item| item.size)
            };
            let add = |volume: &mut Volume, size: Option<u64>| {
                if let Some(size) = size {
                    volume.files += 1;
                    volume.bytes += size;
                }
            };
            match row.action {
                SyncAction::None => {}
                SyncAction::CopyToRight => add(&mut summary.copy_to_right, file(&row.left)),
                SyncAction::CopyToLeft => add(&mut summary.copy_to_left, file(&row.right)),
                SyncAction::DeleteRight => summary.delete_right += 1,
                SyncAction::DeleteLeft => summary.delete_left += 1,
            }
        }
        summary
    }

    /// Changes the action of row `index`.
    ///
    /// # Errors
    ///
    /// `sync_action_invalid`: no such row, or the row does not allow the action
    /// ([`SyncRow::allows`]).
    pub fn set_action(&mut self, index: usize, action: SyncAction) -> Result<(), EngineError> {
        match self.rows.get_mut(index) {
            Some(row) if row.allows(action) => {
                row.action = action;
                Ok(())
            }
            _ => Err(EngineError::SyncActionInvalid { row: index, action }),
        }
    }

    /// What `compare.dirs` answers: the summary and at most `limit` rows that pass `show`.
    #[must_use]
    pub fn comparison(&self, show: RowFilter, limit: usize) -> DirComparison {
        let mut passing = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| show.passes(row));
        let rows = passing
            .by_ref()
            .take(limit)
            .map(|(index, row)| ShownRow {
                index,
                row: row.clone(),
            })
            .collect();
        DirComparison {
            left: self.left.clone(),
            right: self.right.clone(),
            summary: self.summary(),
            skipped: self.skipped,
            rows,
            truncated: passing.next().is_some(),
        }
    }
}

/// The parameters of `compare.dirs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DirCompareRequest {
    /// The left directory: absolute, or relative to the left panel's; the left panel's when
    /// `None`.
    pub left: Option<String>,
    /// The right directory: absolute, or relative to the right panel's; the right panel's when
    /// `None`.
    pub right: Option<String>,
    pub options: SyncOptions,
    /// Which rows the answer lists.
    pub show: RowFilter,
    /// At most this many rows in the answer.
    pub limit: usize,
}

impl Default for DirCompareRequest {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            options: SyncOptions::default(),
            show: RowFilter::default(),
            limit: DIR_COMPARE_LIMIT,
        }
    }
}

/// A row as `compare.dirs` lists it, with its index in the whole plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownRow {
    pub index: usize,
    #[serde(flatten)]
    pub row: SyncRow,
}

/// What `compare.dirs` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirComparison {
    pub left: VfsPath,
    pub right: VfsPath,
    pub summary: SyncSummary,
    pub skipped: u64,
    pub rows: Vec<ShownRow>,
    /// More rows passed the filter than the limit let through.
    pub truncated: bool,
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
