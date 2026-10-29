//! The `compare` tool group: two files by content, two directory trees as a proposed
//! synchronization (both only read), and the synchronization carried out as jobs of the
//! operations queue (`dirs_sync_apply`). Deleting or replacing files loses data: a call that
//! allows either is gated as `destructive` (`Command::effective_class`) and needs a confirmation
//! code.

use rc_engine::{
    Command, Outcome, PanelId, VfsPath,
    compare::{FileCompareRequest, TextOptions, UNIFIED_LIMIT_MAX},
    sync::{
        DIR_COMPARE_LIMIT, DirCompareRequest, DirComparison, ItemKind, PairState, RowFilter,
        ShownRow, SyncAction, SyncItem, SyncMode, SyncOptions, SyncSummary,
    },
    unix_seconds,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

mod apply;

pub(crate) use self::apply::DirsSyncApplyParams;
use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, json_result, respond},
};

/// The most rows one `dirs_compare` answer lists.
const ROWS_MAX: usize = 5000;

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FilesCompareParams {
    /// The left file: absolute, or relative to the left panel's directory. Omit left and right
    /// to compare the two selected files of the active panel, or else the files under both
    /// panels' cursors.
    #[serde(default)]
    pub left: Option<String>,
    /// The right file: absolute, or relative to the right panel's directory.
    #[serde(default)]
    pub right: Option<String>,
    /// Lines that differ only in white space count as equal.
    #[serde(default)]
    pub ignore_whitespace: bool,
    /// Lines that differ only in case count as equal.
    #[serde(default)]
    pub ignore_case: bool,
    /// Lines that differ only in their ending (LF, CRLF, CR, none) count as equal.
    #[serde(default)]
    pub ignore_line_endings: bool,
    /// Include a unified diff of at most this many bytes (up to 1048576); none when omitted.
    #[serde(default)]
    pub unified_limit: Option<usize>,
}

/// Which way a synchronization goes.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModeArg {
    Both,
    LeftToRight,
    RightToLeft,
    MirrorToRight,
    MirrorToLeft,
}

impl From<ModeArg> for SyncMode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::Both => Self::Both,
            ModeArg::LeftToRight => Self::LeftToRight,
            ModeArg::RightToLeft => Self::RightToLeft,
            ModeArg::MirrorToRight => Self::MirrorToRight,
            ModeArg::MirrorToLeft => Self::MirrorToLeft,
        }
    }
}

/// Which rows to list.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShowArg {
    Differences,
    All,
    Equal,
    LeftOnly,
    RightOnly,
}

impl From<ShowArg> for RowFilter {
    fn from(show: ShowArg) -> Self {
        match show {
            ShowArg::Differences => Self::Differences,
            ShowArg::All => Self::All,
            ShowArg::Equal => Self::Equal,
            ShowArg::LeftOnly => Self::LeftOnly,
            ShowArg::RightOnly => Self::RightOnly,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DirsCompareParams {
    /// The left directory: absolute, or relative to the left panel's; the left panel's when
    /// omitted.
    #[serde(default)]
    pub left: Option<String>,
    /// The right directory: absolute, or relative to the right panel's; the right panel's when
    /// omitted.
    #[serde(default)]
    pub right: Option<String>,
    /// both (default), left_to_right, right_to_left, or mirror_to_right / mirror_to_left (the
    /// target becomes a copy of the source, deletes included).
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
    /// Total Commander masks: `*.rs *.toml | *.bak target\` (after | excludes; a trailing \ or
    /// / makes a directory mask).
    #[serde(default)]
    pub masks: Option<String>,
    /// differences (default), all, equal, left_only or right_only.
    #[serde(default)]
    pub show: Option<ShowArg>,
    /// At most this many rows (default 1000, up to 5000).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SelectDifferencesParams {
    /// Files of the same size are also compared byte by byte (keys and credential files only
    /// by size and time).
    #[serde(default)]
    pub by_content: bool,
}

/// Both panels' selections after `panel_select_differences`.
#[derive(Serialize)]
struct Selections {
    left: Vec<String>,
    right: Vec<String>,
}

/// A directory comparison as tools answer it: times as Unix seconds, paths as text.
#[derive(Serialize)]
struct DirsView {
    left: VfsPath,
    right: VfsPath,
    summary: SyncSummary,
    #[serde(skip_serializing_if = "is_zero")]
    skipped: u64,
    rows: Vec<RowView>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
}

#[derive(Serialize)]
struct RowView {
    index: usize,
    path: String,
    state: PairState,
    action: SyncAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    left: Option<ItemView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    right: Option<ItemView>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    partial: bool,
}

#[derive(Serialize)]
struct ItemView {
    kind: ItemKind,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_unix: Option<u64>,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl From<SyncItem> for ItemView {
    fn from(item: SyncItem) -> Self {
        Self {
            kind: item.kind,
            size: item.size,
            modified_unix: item.modified.and_then(unix_seconds),
        }
    }
}

impl From<ShownRow> for RowView {
    fn from(shown: ShownRow) -> Self {
        let row = shown.row;
        Self {
            index: shown.index,
            path: row.path.to_string_lossy().into_owned(),
            state: row.state,
            action: row.action,
            left: row.left.map(ItemView::from),
            right: row.right.map(ItemView::from),
            partial: row.partial,
        }
    }
}

impl From<DirComparison> for DirsView {
    fn from(comparison: DirComparison) -> Self {
        Self {
            left: comparison.left,
            right: comparison.right,
            summary: comparison.summary,
            skipped: comparison.skipped,
            rows: comparison.rows.into_iter().map(RowView::from).collect(),
            truncated: comparison.truncated,
        }
    }
}

#[tool_router(router = compare_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Compare two files by content (Total Commander's Compare by content). Answers identical, both sizes, first_difference (offset of the first differing byte and both byte values; null for a file that ended there) and, for two text files up to 16 MiB and 1,000,000 lines that differ, text: the encodings, equal (no line differs under the ignore options), changes (blocks of differing lines), first_change (line numbers) and with unified_limit a unified diff, truncated past that many bytes. Files of any size are compared streamed. Keys and credential files are refused with sensitive_path, also when picked from the panels. Errors: compare_needs_files, is_a_directory, not_found, permission_denied, sensitive_path."
    )]
    pub(crate) async fn files_compare(
        &self,
        Parameters(params): Parameters<FilesCompareParams>,
    ) -> ToolResult {
        let request = FileCompareRequest {
            left: params.left,
            right: params.right,
            options: TextOptions {
                ignore_whitespace: params.ignore_whitespace,
                ignore_case: params.ignore_case,
                ignore_line_endings: params.ignore_line_endings,
            },
            unified_limit: params.unified_limit.unwrap_or(0).min(UNIFIED_LIMIT_MAX),
        };
        self.dispatch_mcp(Command::CompareFiles(request)).await
    }

    #[tool(
        description = "Compare two directory trees (Total Commander's Synchronize dirs) and propose a synchronization; changes nothing. Each row has the path below both roots, left and right (kind, size, modified_unix; absent where that side has none), state (equal, left_only, right_only, left_newer, right_newer, different = same time but other size or content, kind_mismatch) and the action the mode proposes (none, copy_to_right, copy_to_left, delete_right, delete_left; only the mirror modes delete or overwrite a newer file). index is the row's place in the whole plan; summary counts every row and the files and bytes to copy each way. A directory row is partial when masks or links left something below it out. by_content never reads keys and credential files: their pairs are judged by size and time. Errors: not_found, not_a_directory, permission_denied."
    )]
    pub(crate) async fn dirs_compare(
        &self,
        Parameters(params): Parameters<DirsCompareParams>,
    ) -> ToolResult {
        let defaults = SyncOptions::default();
        let request = DirCompareRequest {
            left: params.left,
            right: params.right,
            options: SyncOptions {
                mode: params.mode.map_or(defaults.mode, Into::into),
                subdirs: params.subdirs.unwrap_or(defaults.subdirs),
                by_content: params.by_content,
                ignore_date: params.ignore_date,
                time_tolerance_secs: params
                    .time_tolerance_secs
                    .unwrap_or(defaults.time_tolerance_secs),
                ignore_dst_hour: params.ignore_dst_hour,
                masks: params.masks.unwrap_or_default(),
            },
            show: params.show.map(Into::into).unwrap_or_default(),
            limit: params.limit.unwrap_or(DIR_COMPARE_LIMIT).min(ROWS_MAX),
        };
        match self
            .context
            .engine
            .dispatch_mcp_cancellable(Command::CompareDirs(request), super::call_cancel())
            .await
        {
            Ok(Outcome::DirComparison(comparison)) => json_result(&DirsView::from(comparison)),
            Ok(other) => {
                tracing::warn!(?other, "compare.dirs answered something unexpected");
                Err(rmcp::ErrorData::internal_error(
                    "the command answered something this tool does not expect",
                    None,
                ))
            }
            Err(error) => Ok(error_result(error.code(), &error.to_string())),
        }
    }

    #[tool(
        description = "Total Commander's Compare directories (Shift+F2): compares the files of the two panels' folders by name, size and time (2 s tolerance; by_content: files of one size byte by byte) and replaces each panel's selection with its files that the other side lacks, has older, or has in another size at the same time. Folders are not compared; with the setting selection.include_folders on (settings_set), each panel also selects its folders the other side lacks. Answers both selections; copy them with ops_copy. Errors: not_found, not_a_directory, permission_denied."
    )]
    pub(crate) async fn panel_select_differences(
        &self,
        Parameters(params): Parameters<SelectDifferencesParams>,
    ) -> ToolResult {
        let engine = &self.context.engine;
        let selected = engine
            .dispatch_mcp_cancellable(
                Command::SelectDifferences {
                    by_content: params.by_content,
                },
                super::call_cancel(),
            )
            .await
            .map(|_| Selections {
                left: engine.panel(PanelId::Left).selection.into_iter().collect(),
                right: engine.panel(PanelId::Right).selection.into_iter().collect(),
            });
        respond(selected)
    }

    #[tool(
        description = "Carry out a synchronization of two directory trees (Total Commander's Synchronize dirs): compares them again with the dirs_compare options, starts from the proposed actions (proposed=false: from none), sets the given actions by row path, and queues copy and delete jobs of the operations queue; answers the jobs, follow them with ops_list. Only what is there now is copied or deleted. Files that exist on the target side are skipped unless overwrite=true; a plan that deletes runs nothing unless allow_deletes=true (recycle bin, or permanent=true). Either flag needs a confirmation code. Keys and credential files are never copied: sensitive_path, and no job runs. A tree that is or would put an entry into rCommander's data folder, an SSH folder or an autostart folder, or onto a shell's start-up file, is refused with protected_target. Errors: sync_deletes_unconfirmed, sync_row_not_found, sync_action_invalid, sensitive_path, protected_target, archive_read_only, not_found, not_a_directory, permission_denied."
    )]
    pub(crate) async fn dirs_sync_apply(
        &self,
        Parameters(params): Parameters<DirsSyncApplyParams>,
    ) -> ToolResult {
        self.context
            .engine
            .pin_mcp_panels(async {
                // Either tree may be copied into the other: both roots and every entry of each
                // as it would land in the other pass the write guard (RC-080-09), the roots
                // against the folders pinned for the call (RC-080-42).
                let engine = &self.context.engine;
                let root = |panel: PanelId, given: Option<&str>| match given {
                    Some(dir) => engine.resolve_target(Some(panel), dir),
                    None => engine.mcp_folder(panel),
                };
                let left = root(PanelId::Left, params.left.as_deref());
                let right = root(PanelId::Right, params.right.as_deref());
                let guard = self.write_guard();
                for (from, to) in [(&left, &right), (&right, &left)] {
                    if let Some(refusal) = guard.refuse_copy(from, to).await {
                        return Ok(refusal);
                    }
                }
                self.dispatch_mcp(Command::SyncApply(params.request()))
                    .await
            })
            .await
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
