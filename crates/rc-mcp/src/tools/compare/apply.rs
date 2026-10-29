//! `dirs_sync_apply`'s parameters as a `sync.apply` request; a call that may delete or overwrite
//! is `destructive` (`Command::effective_class`, RC-080-42).

use rc_engine::{
    ConflictChoice, DeleteMode,
    sync::{RowAction, SyncAction, SyncApplyRequest, SyncOptions, SyncRun},
};
use rmcp::schemars;
use serde::Deserialize;

use super::ModeArg;
use crate::tools::ops::ErrorArg;

/// An action a row can take.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionArg {
    None,
    CopyToRight,
    CopyToLeft,
    DeleteRight,
    DeleteLeft,
}

impl From<ActionArg> for SyncAction {
    fn from(action: ActionArg) -> Self {
        match action {
            ActionArg::None => Self::None,
            ActionArg::CopyToRight => Self::CopyToRight,
            ActionArg::CopyToLeft => Self::CopyToLeft,
            ActionArg::DeleteRight => Self::DeleteRight,
            ActionArg::DeleteLeft => Self::DeleteLeft,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct RowActionArg {
    /// The row's path below both roots, as dirs_compare lists it.
    pub path: String,
    pub action: ActionArg,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct DirsSyncApplyParams {
    /// The left directory: absolute, or relative to the left panel's; the left panel's when
    /// omitted.
    #[serde(default)]
    pub left: Option<String>,
    /// The right directory: absolute, or relative to the right panel's; the right panel's when
    /// omitted.
    #[serde(default)]
    pub right: Option<String>,
    /// As for dirs_compare: both (default), left_to_right, right_to_left, mirror_to_right,
    /// mirror_to_left.
    #[serde(default)]
    pub mode: Option<ModeArg>,
    /// Descend into subdirectories (default true).
    #[serde(default)]
    pub subdirs: Option<bool>,
    /// Compare files of the same size byte by byte.
    #[serde(default)]
    pub by_content: bool,
    /// Compare by size (and content) only, not by time.
    #[serde(default)]
    pub ignore_date: bool,
    /// Times this many seconds apart count as equal (default 2, for FAT).
    #[serde(default)]
    pub time_tolerance_secs: Option<u32>,
    /// Times exactly one hour apart count as equal (a daylight saving shift).
    #[serde(default)]
    pub ignore_dst_hour: bool,
    /// Total Commander masks, as for dirs_compare.
    #[serde(default)]
    pub masks: Option<String>,
    /// Start from the actions the mode proposes (default true); false: only the rows named in
    /// actions act.
    #[serde(default)]
    pub proposed: Option<bool>,
    /// Actions by row path, over the start.
    #[serde(default)]
    pub actions: Vec<RowActionArg>,
    /// Let the deletes run, into the recycle bin unless permanent (needs a confirmation code).
    /// Without it a plan that deletes runs nothing: sync_deletes_unconfirmed.
    #[serde(default)]
    pub allow_deletes: bool,
    /// Delete for good instead of into the recycle bin.
    #[serde(default)]
    pub permanent: bool,
    /// Replace the files that exist on the target side - the newer or different ones the plan
    /// copies over (needs a confirmation code). Without it they are skipped.
    #[serde(default)]
    pub overwrite: bool,
    /// When a file fails: skip (default) or cancel.
    #[serde(default)]
    pub on_error: ErrorArg,
    /// Compare BLAKE3 checksums of source and copy after each file.
    #[serde(default)]
    pub verify: bool,
}

impl DirsSyncApplyParams {
    pub(crate) fn request(self) -> SyncApplyRequest {
        let defaults = SyncOptions::default();
        SyncApplyRequest {
            left: self.left,
            right: self.right,
            options: SyncOptions {
                mode: self.mode.map_or(defaults.mode, Into::into),
                subdirs: self.subdirs.unwrap_or(defaults.subdirs),
                by_content: self.by_content,
                ignore_date: self.ignore_date,
                time_tolerance_secs: self
                    .time_tolerance_secs
                    .unwrap_or(defaults.time_tolerance_secs),
                ignore_dst_hour: self.ignore_dst_hour,
                masks: self.masks.unwrap_or_default(),
            },
            proposed: self.proposed.unwrap_or(true),
            actions: self
                .actions
                .into_iter()
                .map(|wanted| RowAction {
                    path: wanted.path.into(),
                    action: wanted.action.into(),
                })
                .collect(),
            // An MCP job never asks: every conflict is decided here.
            run: SyncRun {
                deletes_confirmed: self.allow_deletes,
                delete_mode: if self.permanent {
                    DeleteMode::Permanent
                } else {
                    DeleteMode::Trash
                },
                on_conflict: Some(if self.overwrite {
                    ConflictChoice::Overwrite
                } else {
                    ConflictChoice::Skip
                }),
                on_error: self.on_error.into(),
                verify: self.verify,
            },
        }
    }
}
