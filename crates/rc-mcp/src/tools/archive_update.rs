//! The tools that change an archive (RC-110-05): thin wrappers over `archive.add` and
//! `archive.delete`, archive update jobs of the operations queue like `archives_pack`.
//!
//! An MCP job never asks: a name that exists in the archive follows `on_conflict` (skip by
//! default; overwrite needs a confirmation code, and so does deleting the added sources).
//! `archives_delete` is destructive: entries go for good, there is no recycle bin inside an
//! archive. An archive with encrypted entries is never changed (`archive_encrypted`) - no tool
//! takes a password. What is added makes a readable copy, so sources that are or hold keys and
//! credentials are refused with `sensitive_path`, as for packing; the archive file itself passes
//! the write guard (`guard_write.rs`, RC-080-09) like the archive `archives_pack` writes.

use rc_engine::{Command, ErrorPolicy, JobOptions, PanelId, UpdateSettings, VfsPath, archive_file};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use super::{
    archive_ops::{named, refuse_sensitive_sources},
    ops::{ConflictArg, ErrorArg},
    panels::PanelArg,
};
use crate::{RcMcpServer, answer::ToolResult, guard_write::other_panel_target};

fn yes() -> bool {
    true
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct AddParams {
    /// Entries to add: absolute, or relative to the panel's directory. Omitted or empty: the
    /// panel's selection, else the entry under its cursor.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Where they go: a folder inside an archive (`C:\data\a.zip\docs`) or an archive file (its
    /// root), absolute or relative to the other panel's directory. Omitted: the other panel's
    /// folder, which must lie inside an archive.
    #[serde(default)]
    pub target: Option<String>,
    /// The panel the sources are in; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// 0 stores, 9 packs hardest; 6 when omitted.
    #[serde(default)]
    pub level: Option<u8>,
    /// 7z: the new files in one solid block (default) instead of one per file.
    #[serde(default = "yes")]
    pub solid: bool,
    /// Add folders with everything inside (default).
    #[serde(default = "yes")]
    pub recurse: bool,
    /// Keep the folders in the names (default); false stores bare file names.
    #[serde(default = "yes")]
    pub keep_paths: bool,
    /// Remove the added files once the archive is replaced (needs a confirmation code).
    #[serde(default)]
    pub delete_sources: bool,
    /// Store links as links (tar, zip); false adds the files they lead to.
    #[serde(default)]
    pub store_links: bool,
    /// When a name exists in the archive: skip (default), overwrite* (needs a confirmation
    /// code), cancel; the rename choices skip too, a name inside an archive is not changed.
    #[serde(default)]
    pub on_conflict: ConflictArg,
    /// When a file fails: skip (default) or cancel.
    #[serde(default)]
    pub on_error: ErrorArg,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct DeleteParams {
    /// Names in the panel's folder inside an archive. Omitted or empty: the panel's selection,
    /// else the entry under its cursor.
    #[serde(default)]
    pub entries: Vec<String>,
    /// The panel showing the archive; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

impl AddParams {
    /// The `archive.add` command for `sources`, the ones the call names or the panel's.
    pub(crate) fn command(&self, sources: Vec<String>) -> Command {
        let defaults = UpdateSettings::default();
        Command::ArchiveAdd {
            panel: self.panel.map(Into::into),
            sources,
            target: self.target.clone(),
            settings: UpdateSettings {
                level: self.level.unwrap_or(defaults.level).min(9),
                solid: self.solid,
                recurse: self.recurse,
                keep_paths: self.keep_paths,
                delete_sources: self.delete_sources,
                store_links: self.store_links,
                // The engine takes it from `archive.verify_after_update`.
                verify: defaults.verify,
            },
            options: JobOptions {
                on_conflict: Some(self.on_conflict.into()),
                on_error: self.on_error.into(),
                verify: false,
                ..JobOptions::default()
            },
        }
    }
}

impl DeleteParams {
    /// The `archive.delete` command for `entries`, the ones the call names or the panel's.
    pub(crate) fn command(&self, entries: Vec<String>) -> Command {
        Command::ArchiveDelete {
            panel: self.panel.map(Into::into),
            entries,
            options: JobOptions {
                on_conflict: None,
                // The archive is one whole: a failure stops the job, it never asks.
                on_error: ErrorPolicy::Cancel,
                verify: false,
                ..JobOptions::default()
            },
        }
    }
}

/// The archive file a place inside an archive lies in; any other place as it is.
fn the_file(place: VfsPath) -> VfsPath {
    archive_file(&place).cloned().unwrap_or(place)
}

#[tool_router(router = archive_update_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Add files and folders to an existing zip, 7z or plain tar archive - into a folder inside it - as a background job of the operations queue (Total Commander's F5 into an archive); answers at once with the queued job, follow it with ops_list. target is a folder inside an archive (C:\\data\\a.zip\\docs) or an archive file; omitted, the other panel's folder, which must lie inside an archive. The archive is written anew beside itself, read back, and replaces the old one only when complete; kept entries are copied as stored. A name that exists follows on_conflict (skip by default; overwrite needs a confirmation code, as does delete_sources). Errors: archive_not_modifiable (tar.gz, tar.xz, tar.bz2, tar.zst, single compressed files, a self-extractor), archive_encrypted, archive_not_local (inside another archive or on a server), archive_file_read_only, not_enough_space, archive_changed (changed meanwhile by something else), archive_in_use, not_an_archive, sensitive_path, protected_target."
    )]
    pub(crate) async fn archives_add(
        &self,
        Parameters(params): Parameters<AddParams>,
    ) -> ToolResult {
        let engine = &self.context.engine;
        engine
            .pin_mcp_panels(async {
                let panel = Some(
                    params
                        .panel
                        .map_or_else(|| engine.active_panel(), Into::into),
                );
                let sources = named(engine, panel, params.sources.clone());
                if let Some(refusal) = refuse_sensitive_sources(engine, panel, &sources).await {
                    return Ok(refusal);
                }
                let target = other_panel_target(engine, panel, params.target.as_deref());
                if let Some(refusal) = self.write_guard().refuse(&the_file(target)).await {
                    return Ok(refusal);
                }
                let mut command = params.command(sources);
                if let Command::ArchiveAdd { panel: fixed, .. } = &mut command {
                    *fixed = panel;
                }
                self.dispatch_mcp(command).await
            })
            .await
    }

    #[tool(
        description = "Remove entries from a zip, 7z or plain tar archive for good, as a background job of the operations queue (Total Commander's F8 inside an archive): names in the folder the panel shows inside the archive, a folder with everything below it; needs a confirmation code, there is no recycle bin inside an archive. Answers at once with the queued job, follow it with ops_list. The archive is written anew beside itself and replaces the old one only when complete. Errors: not_an_archive (the panel shows no archive), archive_not_modifiable, archive_encrypted, archive_not_local, archive_file_read_only, not_enough_space, archive_changed, archive_in_use, protected_target."
    )]
    pub(crate) async fn archives_delete(
        &self,
        Parameters(params): Parameters<DeleteParams>,
    ) -> ToolResult {
        let engine = &self.context.engine;
        engine
            .pin_mcp_panels(async {
                let id: PanelId = params
                    .panel
                    .map_or_else(|| engine.active_panel(), Into::into);
                let panel = Some(id);
                let entries = named(engine, panel, params.entries.clone());
                let file = the_file(engine.mcp_folder(id));
                if let Some(refusal) = self.write_guard().refuse(&file).await {
                    return Ok(refusal);
                }
                let mut command = params.command(entries);
                if let Command::ArchiveDelete { panel: fixed, .. } = &mut command {
                    *fixed = panel;
                }
                self.dispatch_mcp(command).await
            })
            .await
    }
}

#[cfg(test)]
#[path = "archive_update_tests.rs"]
mod tests;
