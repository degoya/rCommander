//! The archive tools (RC-030-03, RC-030-04): thin wrappers over `archive.extract` and
//! `archive.pack`, jobs of the operations queue like `ops_copy`.
//!
//! An MCP job never asks: conflicts and failing files follow the presets, and an encrypted
//! archive fails its entries with `password_required` - no tool takes or returns a password
//! (`archive.password` is GUI only). Overwriting, deleting the packed sources and replacing an
//! archive lose data, so those calls are gated as `destructive` (`Command::effective_class`,
//! RC-080-42).
//!
//! Packing makes a readable copy of everything it packs, so a source that is sensitive, or a
//! folder holding a sensitive file anywhere below it (followed links included), refuses the
//! whole call with `sensitive_path` - without naming which file, so the refusal is no oracle
//! (audit 2026-10-17, H2 and M1). An archive to unpack is judged like a file to read.
//!
//! Where they write passes the write guard (`guard_write.rs`, RC-080-09): the archive to pack,
//! and the folder to unpack into with every entry as it lands there - an archive holding
//! `.ssh/authorized_keys` or `Startup/run.bat` is refused with `protected_target`.

use std::path::{Path, PathBuf};

use rc_engine::{
    Command, Engine, ExtractSettings, JobOptions, PackFormat, PackSettings, PanelId, VfsPath,
};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;

use super::{
    ops::{ConflictArg, ErrorArg},
    panels::PanelArg,
};
use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
    guard::{refuse_sensitive, sensitive_path},
    guard_write::{Unpack, other_panel_target},
};

/// The archive format of `archives_pack`.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FormatArg {
    #[default]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
}

impl From<FormatArg> for PackFormat {
    fn from(arg: FormatArg) -> Self {
        match arg {
            FormatArg::Zip => Self::Zip,
            FormatArg::SevenZ => Self::SevenZ,
            FormatArg::Tar => Self::Tar,
            FormatArg::TarGz => Self::TarGz,
            FormatArg::TarXz => Self::TarXz,
            FormatArg::TarBz2 => Self::TarBz2,
            FormatArg::TarZst => Self::TarZst,
        }
    }
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct ExtractParams {
    /// Archives: absolute, or relative to the panel's directory. Omitted or empty: the panel's
    /// selection, else the entry under its cursor.
    #[serde(default)]
    pub archives: Vec<String>,
    /// The directory to unpack into (created when missing): absolute, or relative to the other
    /// panel's directory. Omitted: the other panel's directory.
    #[serde(default)]
    pub target: Option<String>,
    /// The panel the archives are in; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// Each archive into a folder of its own, named after it.
    #[serde(default)]
    pub own_folder: bool,
    /// Only files whose names match: `*.*` (default), `*.txt;*.md`, `*|*.tmp`.
    #[serde(default)]
    pub mask: Option<String>,
    /// Keep the archive's folders (default); false puts every file straight into the target.
    #[serde(default = "yes")]
    pub keep_paths: bool,
    /// When a file exists: skip (default), rename, rename_existing, overwrite*, cancel.
    #[serde(default)]
    pub on_conflict: ConflictArg,
    /// When a file fails: skip (default) or cancel.
    #[serde(default)]
    pub on_error: ErrorArg,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct PackParams {
    /// Entries to pack: absolute, or relative to the panel's directory. Omitted or empty: the
    /// panel's selection, else the entry under its cursor.
    #[serde(default)]
    pub sources: Vec<String>,
    /// The archive to write (the directory with each_own_archive): absolute, or relative to
    /// the other panel's directory. Omitted: named after the source in the other panel.
    #[serde(default)]
    pub target: Option<String>,
    /// The panel the sources are in; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// zip (default), 7z, tar, tar_gz, tar_xz, tar_bz2 or tar_zst.
    #[serde(default)]
    pub format: FormatArg,
    /// 0 stores, 9 packs hardest; 6 when omitted.
    #[serde(default)]
    pub level: Option<u8>,
    /// 7z: one solid block (default) instead of one per file.
    #[serde(default = "yes")]
    pub solid: bool,
    /// Pack folders with everything inside (default).
    #[serde(default = "yes")]
    pub recurse: bool,
    /// Keep the folders in the names (default); false stores bare file names.
    #[serde(default = "yes")]
    pub keep_paths: bool,
    /// One archive per source, named after it, in the target directory.
    #[serde(default)]
    pub each_own_archive: bool,
    /// Remove the packed files once the archive is complete (needs a confirmation code).
    #[serde(default)]
    pub delete_sources: bool,
    /// Store links as links (tar, zip); false packs the files they lead to.
    #[serde(default)]
    pub store_links: bool,
    /// Replace an existing archive (needs a confirmation code); otherwise already_exists.
    #[serde(default)]
    pub overwrite: bool,
    /// When a file fails: skip (default) or cancel.
    #[serde(default)]
    pub on_error: ErrorArg,
}

impl ExtractParams {
    /// The `archive.extract` command for `archives`, the ones the call names or the panel's.
    pub(crate) fn command(&self, archives: Vec<String>) -> Command {
        let defaults = ExtractSettings::default();
        Command::ArchiveExtract {
            panel: self.panel.map(Into::into),
            archives,
            target: self.target.clone(),
            settings: ExtractSettings {
                own_folder: self.own_folder,
                mask: self.mask.clone().unwrap_or(defaults.mask),
                keep_paths: self.keep_paths,
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

impl PackParams {
    /// The `archive.pack` command for `sources`, the ones the call names or the panel's.
    pub(crate) fn command(&self, sources: Vec<String>) -> Command {
        let defaults = PackSettings::default();
        Command::ArchivePack {
            panel: self.panel.map(Into::into),
            sources,
            target: self.target.clone(),
            settings: PackSettings {
                format: self.format.into(),
                level: self.level.unwrap_or(defaults.level).min(9),
                solid: self.solid,
                recurse: self.recurse,
                keep_paths: self.keep_paths,
                each_own_archive: self.each_own_archive,
                delete_sources: self.delete_sources,
                store_links: self.store_links,
                overwrite: self.overwrite,
            },
            options: JobOptions {
                on_conflict: None,
                on_error: self.on_error.into(),
                verify: false,
                ..JobOptions::default()
            },
        }
    }
}

/// The entries a call names, or the panel's selection, else its cursor: fixed before the guard
/// judges them, so the job packs exactly what was judged.
pub(crate) fn named(engine: &Engine, panel: Option<PanelId>, given: Vec<String>) -> Vec<String> {
    if !given.is_empty() {
        return given;
    }
    let state = engine.panel(panel.unwrap_or_else(|| engine.active_panel()));
    if !state.selection.is_empty() {
        return state.selection.iter().cloned().collect();
    }
    state
        .cursor_entry()
        .map(|entry| vec![entry.name.clone()])
        .unwrap_or_default()
}

/// Whether anything at or below `path` is sensitive, links judged where they lead too.
async fn holds_sensitive(path: &Path) -> bool {
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(at) = stack.pop() {
        if sensitive_path(&at).is_some() {
            return true;
        }
        let Ok(meta) = tokio::fs::symlink_metadata(&at).await else {
            continue;
        };
        if meta.file_type().is_symlink() {
            if let Ok(real) = tokio::fs::canonicalize(&at).await
                && (sensitive_path(&real).is_some()
                    || sensitive_path(&real.join("rcommander-probe")).is_some())
            {
                return true;
            }
            continue;
        }
        if !meta.is_dir() {
            continue;
        }
        if sensitive_path(&at.join("rcommander-probe")).is_some() {
            return true;
        }
        let Ok(mut entries) = tokio::fs::read_dir(&at).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            stack.push(entry.path());
        }
    }
    false
}

/// The refusal when a source to pack is sensitive or holds a sensitive file below it.
async fn refuse_sensitive_sources(
    engine: &Engine,
    panel: Option<PanelId>,
    sources: &[String],
) -> Option<CallToolResult> {
    for source in sources {
        let target = engine.resolve_target(panel, source);
        let path = &target.path;
        // Only a local folder is walked here; another backend's sources are judged by name.
        let local = target.backend == VfsPath::local(path).backend;
        let sensitive = if local {
            holds_sensitive(path).await
        } else {
            sensitive_path(path).is_some()
        };
        if sensitive {
            return Some(error_result(
                "sensitive_path",
                &format!(
                    "{} is or holds files whose contents never leave through MCP (keys, \
                     credentials, rCommander's own data), so it is not packed. A person can \
                     pack it in rCommander.",
                    path.display()
                ),
            ));
        }
    }
    None
}

#[tool_router(router = archive_ops_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Unpack zip, 7z and tar archives (plain, gz, xz, bz2, zst, and single compressed files) into a directory, as a background job of the operations queue; answers at once with the queued job, follow it with ops_list. own_folder puts each archive into a folder named after it; mask picks files by name; keep_paths=false drops the folders. Entries whose names would leave the target (zip slip, absolute, drive or UNC names) and links stored in the archive are never written: they are listed in failures (unsafe_entry, link_not_extracted). Encrypted entries fail with password_required: no tool takes a password, a person unpacks them in rCommander. Existing files follow on_conflict (skip by default; overwrite needs a confirmation code). A target in rCommander's data folder, an SSH folder or an autostart folder, or an entry that would land in one or on a shell's start-up file, is refused with protected_target before anything is unpacked. Errors: not_an_archive, archive_limit, archive_damaged, archive_decoder_memory (an xz or 7z archive needs more decoder memory than the setting archive.decoder_memory_mib allows), sensitive_path, protected_target."
    )]
    pub(crate) async fn archives_extract(
        &self,
        Parameters(params): Parameters<ExtractParams>,
    ) -> ToolResult {
        let engine = &self.context.engine;
        engine
            .pin_mcp_panels(async {
                // One panel for the guard and the command (RC-080-42).
                let panel = Some(
                    params
                        .panel
                        .map_or_else(|| engine.active_panel(), Into::into),
                );
                let archives = named(engine, panel, params.archives.clone());
                for archive in &archives {
                    if let Some(refusal) = refuse_sensitive(engine, panel, archive).await {
                        return Ok(refusal);
                    }
                }
                let dest = other_panel_target(engine, panel, params.target.as_deref());
                let guard = self.write_guard();
                if let Some(refusal) = guard.refuse(&dest).await {
                    return Ok(refusal);
                }
                let unpack = Unpack {
                    dest: &dest,
                    own_folder: params.own_folder,
                    keep_paths: params.keep_paths,
                };
                for archive in &archives {
                    if let Some(refusal) = guard.refuse_unpack(engine, panel, archive, unpack).await
                    {
                        return Ok(refusal);
                    }
                }
                let mut command = params.command(archives);
                if let Command::ArchiveExtract { panel: fixed, .. } = &mut command {
                    *fixed = panel;
                }
                self.dispatch_mcp(command).await
            })
            .await
    }

    #[tool(
        description = "Pack files and folders into a new archive, as a background job of the operations queue; answers at once with the queued job, follow it with ops_list. format zip (default, deflate), 7z (LZMA2, solid by default), tar, tar_gz, tar_xz, tar_bz2 or tar_zst; level 0 (store) to 9. The archive is written under a temporary name and appears only when complete; it is named after the source in the other panel's directory unless target is given. each_own_archive makes one archive per source. An existing archive fails with already_exists unless overwrite is set; overwrite and delete_sources (remove the packed files once the archive is complete) need a confirmation code. The archive must be on a local drive. Sources that are or hold keys, credentials or rCommander's own data anywhere below them are refused with sensitive_path; an archive in rCommander's data folder, an SSH folder or an autostart folder, or under one of rCommander's own file names, with protected_target."
    )]
    pub(crate) async fn archives_pack(
        &self,
        Parameters(params): Parameters<PackParams>,
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
                if let Some(refusal) = self.write_guard().refuse(&target).await {
                    return Ok(refusal);
                }
                let mut command = params.command(sources);
                if let Command::ArchivePack { panel: fixed, .. } = &mut command {
                    *fixed = panel;
                }
                self.dispatch_mcp(command).await
            })
            .await
    }
}

#[cfg(test)]
#[path = "archive_ops_tests.rs"]
mod tests;
