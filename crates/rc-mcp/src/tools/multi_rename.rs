//! The `multi_rename` tools: thin wrappers over `multi_rename.*`, Total Commander's
//! multi-rename tool.
//!
//! The preview reads names only. Apply plans again on the engine's side - it never takes a
//! plan from the caller - and renames nothing when the plan has a conflict; a step never
//! replaces an entry, so apply and undo are `write` tools, not `destructive` ones. Every run is
//! kept in the undo log, and multi_rename_undo reverses the latest one.
//!
//! A rename must not carry a file past the sensitive-path guard (`guard.rs`): renaming
//! `~/.ssh/id_ed25519` to `x.txt` and reading that would bypass it, and renaming a file to a key's
//! or a credential file's name would put it where the guard cannot tell it from one. So every
//! old and every new name of an apply or an undo - both as a file and as a folder - is held
//! against the guard before anything is renamed, and one sensitive name refuses the whole run
//! with `sensitive_path`. Both are held against the write guard as well (`guard_write.rs`,
//! RC-080-09): a folder renamed to `autostart` beside `.config` is refused with
//! `protected_target`.
//!
//! The presets are rules saved under a name (Total Commander's saved settings, RC-020-02); they
//! rename nothing and are `write` tools for the settings they change.

use rc_engine::{
    CaseMode, Change, Command, CounterSettings, MultiRenameRequest, Outcome, RenameRule,
    SearchReplace, VfsPath,
};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;

use super::panels::PanelArg;
use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
    guard::refuse_sensitive_tree,
};

/// The case of the whole new name, as tools name it.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CaseArg {
    /// As the masks rendered it.
    #[default]
    Unchanged,
    Lower,
    Upper,
    /// First character upper case, the rest lower case.
    FirstUpper,
    /// First character of every word upper case.
    WordsUpper,
}

impl From<CaseArg> for CaseMode {
    fn from(case: CaseArg) -> Self {
        match case {
            CaseArg::Unchanged => Self::Unchanged,
            CaseArg::Lower => Self::Lower,
            CaseArg::Upper => Self::Upper,
            CaseArg::FirstUpper => Self::FirstUpper,
            CaseArg::WordsUpper => Self::WordsUpper,
        }
    }
}

/// Search and replace after the masks and the case.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub(crate) struct SearchArg {
    /// What to look for. Plain text takes several terms separated by | (ae|oe).
    pub find: String,
    /// What to put in its place: per term separated by | (a|o), or one text for all; with regex
    /// $1, ${name} insert groups and $$ is a literal $.
    pub replace: String,
    /// find is a regular expression (Rust regex syntax).
    pub regex: bool,
    /// Upper and lower case differ.
    pub case_sensitive: bool,
    /// Only the first occurrence.
    pub first_only: bool,
    /// Search in the name with its extension, not only in the name.
    pub include_extension: bool,
    /// When find matches, the whole name becomes replace.
    pub substitute: bool,
}

impl From<SearchArg> for SearchReplace {
    fn from(search: SearchArg) -> Self {
        Self {
            find: search.find,
            replace: search.replace,
            regex: search.regex,
            case_sensitive: search.case_sensitive,
            first_only: search.first_only,
            include_extension: search.include_extension,
            substitute: search.substitute,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct RenameParams {
    /// Entries of the folder, in the order the counter counts them. Omitted or empty: the
    /// panel's selection in panel order, else the entry under its cursor (only without dir).
    #[serde(default)]
    pub names: Vec<String>,
    /// The folder: absolute, or relative to the panel's folder; the panel's own when omitted.
    #[serde(default)]
    pub dir: Option<String>,
    /// The panel dir starts from and whose selection is used; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// The mask for the name without extension, [N] when omitted. Total Commander's
    /// placeholders: [N] name, [N2-5] its characters 2 to 5 ([N3], [N2,5], [N2-], [N-8,5]),
    /// [E] extension, [A] name with extension, [C] counter ([C10+5:3] start 10, step 5, 3
    /// digits), [P] parent and [G] grandparent folder, [Y] [M] [D] [h] [m] [s] the file's date
    /// and time ([YMD], [hms]), [U] [L] [F] [n] case from there on, [[] and []] literal brackets.
    #[serde(default)]
    pub name_mask: Option<String>,
    /// The mask for the extension, [E] when omitted; when it renders nothing the name has no dot.
    #[serde(default)]
    pub extension_mask: Option<String>,
    /// The case of the whole new name: unchanged (default), lower, upper, first_upper or
    /// words_upper.
    #[serde(default)]
    pub case: CaseArg,
    /// Search and replace after the masks and the case.
    #[serde(default)]
    pub search: Option<SearchArg>,
    /// The counter's first value where [C] sets none; 1 by default.
    #[serde(default)]
    pub counter_start: Option<i64>,
    /// The counter's step; 1 by default.
    #[serde(default)]
    pub counter_step: Option<i64>,
    /// The counter's least number of digits; 1 by default.
    #[serde(default)]
    pub counter_width: Option<u32>,
}

/// A rule from the tools' fields, each omitted one at its default.
struct RuleArgs {
    name_mask: Option<String>,
    extension_mask: Option<String>,
    case: CaseArg,
    search: Option<SearchArg>,
    counter_start: Option<i64>,
    counter_step: Option<i64>,
    counter_width: Option<u32>,
}

impl RuleArgs {
    fn rule(self) -> RenameRule {
        let defaults = RenameRule::default();
        let counter = CounterSettings::default();
        RenameRule {
            name_mask: self.name_mask.unwrap_or(defaults.name_mask),
            extension_mask: self.extension_mask.unwrap_or(defaults.extension_mask),
            case: self.case.into(),
            search: self.search.map(Into::into),
            counter: CounterSettings {
                start: self.counter_start.unwrap_or(counter.start),
                step: self.counter_step.unwrap_or(counter.step),
                width: self.counter_width.unwrap_or(counter.width),
            },
        }
    }
}

impl RenameParams {
    fn request(self) -> MultiRenameRequest {
        MultiRenameRequest {
            panel: self.panel.map(Into::into),
            dir: self.dir,
            names: self.names,
            rule: RuleArgs {
                name_mask: self.name_mask,
                extension_mask: self.extension_mask,
                case: self.case,
                search: self.search,
                counter_start: self.counter_start,
                counter_step: self.counter_step,
                counter_width: self.counter_width,
            }
            .rule(),
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct PresetSaveParams {
    /// The preset's name, 1 to 64 characters; a preset of that name (in any case) is replaced.
    pub name: String,
    /// The mask for the name without extension, [N] when omitted; placeholders as in
    /// multi_rename_preview.
    #[serde(default)]
    pub name_mask: Option<String>,
    /// The mask for the extension, [E] when omitted.
    #[serde(default)]
    pub extension_mask: Option<String>,
    /// The case of the whole new name: unchanged (default), lower, upper, first_upper or
    /// words_upper.
    #[serde(default)]
    pub case: CaseArg,
    /// Search and replace after the masks and the case.
    #[serde(default)]
    pub search: Option<SearchArg>,
    /// The counter's first value where [C] sets none; 1 by default.
    #[serde(default)]
    pub counter_start: Option<i64>,
    /// The counter's step; 1 by default.
    #[serde(default)]
    pub counter_step: Option<i64>,
    /// The counter's least number of digits; 1 by default.
    #[serde(default)]
    pub counter_width: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct PresetNameParams {
    /// The preset's name, in any case.
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct PresetRenameParams {
    /// The preset's name, in any case.
    pub name: String,
    /// Its new name, 1 to 64 characters, not another preset's.
    pub new_name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct UndoParams {}

impl RcMcpServer {
    /// The refusal when any of `pairs` - an old and a new name in `dir` - is sensitive, or a
    /// place the write guard protects (RC-080-09: no folder renamed into an autostart folder).
    async fn refuse_sensitive_renames(
        &self,
        dir: &VfsPath,
        pairs: &[(&str, &str)],
    ) -> Option<CallToolResult> {
        let guard = self.write_guard();
        for &(from, to) in pairs {
            for name in [from, to] {
                let target = dir.path.join(name);
                let target = target.to_string_lossy();
                if let Some(refusal) =
                    refuse_sensitive_tree(&self.context.engine, None, &target).await
                {
                    return Some(refusal);
                }
                if let Some(refusal) = guard.refuse(&dir.join(name)).await {
                    return Some(refusal);
                }
            }
        }
        None
    }

    /// The refusal when the latest run of the undo log renamed a sensitive name, or would
    /// rename one back. A log that cannot be read is the engine's to refuse.
    async fn refuse_sensitive_undo(&self) -> Option<CallToolResult> {
        let run = self.context.db.latest_rename_run().await.ok().flatten()?;
        let dir: VfsPath = serde_json::from_str(&run.dir).ok()?;
        // Collected first: no closure is held across the awaits (the tool's future is `Send`).
        let pairs: Vec<(&str, &str)> = run
            .steps
            .iter()
            .map(|step| (step.from.as_str(), step.to.as_str()))
            .collect();
        self.refuse_sensitive_renames(&dir, &pairs).await
    }
}

#[tool_router(router = multi_rename_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Preview a multi-rename (Total Commander's multi-rename tool) without renaming anything: for each entry its old and new name, change (unchanged, rename, case_only) and conflict (duplicate, target_exists, invalid_name with a reason, no_timestamp). Masks, case, search and replace and counter as in multi_rename_apply. Errors: rename_mask_* with the position, rename_regex_invalid, rename_input_too_long, entry_not_found, no_sources."
    )]
    pub(crate) async fn multi_rename_preview(
        &self,
        Parameters(params): Parameters<RenameParams>,
    ) -> ToolResult {
        self.run(Command::MultiRenamePreview(params.request()))
            .await
    }

    #[tool(
        description = "Rename entries of one folder by masks, case, search and replace and counter, as a background job of the operations queue: the plan is made again from the folder as it is, and nothing is renamed when it has a conflict (rename_plan_conflicts - run multi_rename_preview to see which). Swaps, rings and case-only changes go through temporary names; no entry is ever overwritten. Answers with the run and its queued job (follow it with ops_list); the run is kept for multi_rename_undo. Keys and credential files, and new names that would look like one, are refused with sensitive_path; names in rCommander's data folder, an SSH folder or an autostart folder, or of a shell's start-up file, with protected_target. Errors also: rename_no_changes, rename_busy, database_unavailable."
    )]
    pub(crate) async fn multi_rename_apply(
        &self,
        Parameters(params): Parameters<RenameParams>,
    ) -> ToolResult {
        let request = params.request();
        let preview = self
            .context
            .engine
            .dispatch_mcp(Command::MultiRenamePreview(request.clone()))
            .await;
        let (dir, plan) = match preview {
            Ok(Outcome::RenamePreview { dir, plan }) => (dir, plan),
            // The preview answers nothing else.
            other => return respond(other),
        };
        let pairs = plan
            .items
            .iter()
            .filter(|item| item.change != Change::Unchanged)
            .map(|item| (item.old.as_str(), item.new.as_str()))
            .collect::<Vec<_>>();
        if let Some(refusal) = self.refuse_sensitive_renames(&dir, &pairs).await {
            return Ok(refusal);
        }
        // Exactly the entries checked, whatever the panel's selection is by now.
        let checked = MultiRenameRequest {
            dir: Some(dir.path.to_string_lossy().into_owned()),
            names: plan.items.into_iter().map(|item| item.old).collect(),
            ..request
        };
        self.run(Command::MultiRenameApply(checked)).await
    }

    #[tool(
        description = "Reverse the latest multi-rename as far as it got (also one cancelled half way, or broken off by a crash), as a background job. Refused as a whole, renaming nothing, when an entry of it was renamed, removed or its old name taken since (rename_undo_mismatch names them). A run that renamed a key or credential file, or gave a file such a name, is refused with sensitive_path. Errors also: rename_nothing_to_undo, rename_undo_incomplete, rename_busy."
    )]
    pub(crate) async fn multi_rename_undo(
        &self,
        Parameters(_params): Parameters<UndoParams>,
    ) -> ToolResult {
        if let Some(refusal) = self.refuse_sensitive_undo().await {
            return Ok(refusal);
        }
        self.run(Command::MultiRenameUndo).await
    }

    #[tool(
        description = "List the saved multi-rename presets (Total Commander's saved settings of the multi-rename tool), sorted by name, each with its rule (name_mask, extension_mask, case, search, counter), and last: the rule of the latest multi-rename run, if any. Renames nothing."
    )]
    pub(crate) async fn multi_rename_preset_list(&self) -> ToolResult {
        self.run(Command::MultiRenamePresetList).await
    }

    #[tool(
        description = "Save a multi-rename rule as a preset under a name, replacing a preset of that name; the rule's fields as in multi_rename_preview, each omitted one at its default. Renames nothing; answers every preset. Errors: rename_preset_invalid (empty or overlong name, more than 200 presets), rename_input_too_long, database_unavailable."
    )]
    pub(crate) async fn multi_rename_preset_save(
        &self,
        Parameters(params): Parameters<PresetSaveParams>,
    ) -> ToolResult {
        let rule = RuleArgs {
            name_mask: params.name_mask,
            extension_mask: params.extension_mask,
            case: params.case,
            search: params.search,
            counter_start: params.counter_start,
            counter_step: params.counter_step,
            counter_width: params.counter_width,
        }
        .rule();
        self.run(Command::MultiRenamePresetSave {
            name: params.name,
            rule,
        })
        .await
    }

    #[tool(
        description = "Delete a saved multi-rename preset; answers the presets left. Destructive: the person is asked, or headless the first call answers with a confirmation code; call again with confirmation set to it once the person agreed. Errors: rename_preset_not_found."
    )]
    pub(crate) async fn multi_rename_preset_delete(
        &self,
        Parameters(params): Parameters<PresetNameParams>,
    ) -> ToolResult {
        self.run(Command::MultiRenamePresetDelete { name: params.name })
            .await
    }

    #[tool(
        description = "Give a saved multi-rename preset another name; answers every preset. Errors: rename_preset_not_found, rename_preset_invalid (a bad name, or one another preset has)."
    )]
    pub(crate) async fn multi_rename_preset_rename(
        &self,
        Parameters(params): Parameters<PresetRenameParams>,
    ) -> ToolResult {
        self.run(Command::MultiRenamePresetRename {
            name: params.name,
            new_name: params.new_name,
        })
        .await
    }
}

#[cfg(test)]
#[path = "multi_rename_tests.rs"]
mod tests;
