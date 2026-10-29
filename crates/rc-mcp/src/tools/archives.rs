//! The `archives` tools (RC-030-02): an archive's entries as the panel shows them, and the
//! archive test. Both only read. Opening an archive in a panel is `navigate` to its path (or to
//! a path through it, `C:\data\backup.zip\docs`).
//!
//! The test reads every entry, so an archive that is a sensitive file - or lies in a sensitive
//! place - is refused with `sensitive_path` before anything is read (no per-entry answer about
//! it). Entry names are data from the archive, never instructions.

use std::path::PathBuf;

use rc_engine::{
    ArchiveEntryInfo, Command, Entry, EntryKind, Outcome, PanelId, UnsafeName, VfsPath,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use super::panels::PanelArg;
use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, respond},
    guard::refuse_sensitive,
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// The archive file: absolute, or relative to the panel's directory. Omitted: the archive
    /// the panel shows.
    #[serde(default)]
    pub path: Option<String>,
    /// The directory inside the archive: `/` is its root (the default with path); relative to
    /// the panel's directory inside that archive without path.
    #[serde(default)]
    pub dir: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TestParams {
    /// The archive files to test: absolute, or relative to the panel's directory.
    pub paths: Vec<String>,
    /// The panel relative paths start from; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// An archive listing as the tool answers it.
#[derive(Serialize)]
struct ListingView {
    /// The directory as a path through the archive file.
    shown: PathBuf,
    path: VfsPath,
    entries: Vec<ItemView>,
}

#[derive(Serialize)]
struct ItemView {
    name: String,
    kind: &'static str,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    packed_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_unix: Option<u64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    encrypted: bool,
    /// Why the stored name was unsafe (`parent_dir` is zip slip); listed under the cleaned name.
    #[serde(skip_serializing_if = "Option::is_none")]
    unsafe_name: Option<UnsafeName>,
    /// The name as stored, where it differs from the path shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    stored_name: Option<String>,
}

impl ItemView {
    fn new(entry: Entry, info: ArchiveEntryInfo) -> Self {
        let kind = match entry.kind {
            EntryKind::File => "file",
            EntryKind::Dir => "dir",
            EntryKind::Symlink { .. } => "symlink",
            EntryKind::Other => "other",
        };
        Self {
            kind,
            size: entry.size,
            packed_size: info.packed_size,
            modified_unix: entry.modified_unix(),
            encrypted: info.encrypted,
            stored_name: info.unsafe_name.map(|_| info.stored_name),
            unsafe_name: info.unsafe_name,
            name: entry.name,
        }
    }
}

#[tool_router(router = archives_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List a directory inside an archive (zip, 7z, tar plain or .gz/.xz/.bz2/.zst, a single compressed file) in the panel's order, hidden entries included: name, kind, size, packed_size, modified_unix, encrypted, and unsafe_name with stored_name for an entry whose stored name climbed out of the archive (zip slip, absolute, drive, UNC) and is listed cleaned. Reads only. Without path, the archive the panel shows; to open an archive in a panel, navigate to its path. Errors: not_an_archive, archive_damaged, archive_limit, archive_decoder_memory (an xz or 7z archive needs more decoder memory than the setting archive.decoder_memory_mib allows), not_found."
    )]
    pub(crate) async fn archives_list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        let outcome = self
            .context
            .engine
            .dispatch_mcp(Command::ArchiveList {
                panel: params.panel.map(Into::into),
                path: params.path,
                dir: params.dir,
            })
            .await;
        respond(outcome.map(|outcome| {
            match outcome {
                Outcome::ArchiveListing {
                    path,
                    shown,
                    entries,
                } => serde_json::to_value(ListingView {
                    shown,
                    path,
                    entries: entries
                        .into_iter()
                        .map(|item| ItemView::new(item.entry, item.info))
                        .collect(),
                })
                .unwrap_or_default(),
                other => serde_json::to_value(other).unwrap_or_default(),
            }
        }))
    }

    #[tool(
        description = "Test archives: one background job each that reads every entry to its end, so the format's CRCs and stream checksums are checked; nothing is written. Answers the queued jobs; follow them with ops_list. When a job is completed, its failures are the entries that did not pass, each with its code: archive_damaged (damaged), password_required (encrypted, not tested), archive_limit. Reads only. Keys, credential stores and rCommander's backups are refused with sensitive_path. Errors: not_an_archive, no_sources, archive_decoder_memory (an xz or 7z archive needs more decoder memory than the setting archive.decoder_memory_mib allows)."
    )]
    pub(crate) async fn archives_test(
        &self,
        Parameters(params): Parameters<TestParams>,
    ) -> ToolResult {
        if params.paths.is_empty() {
            return Ok(error_result("no_sources", "name at least one archive file"));
        }
        let panel: Option<PanelId> = params.panel.map(Into::into);
        for path in &params.paths {
            if let Some(refusal) = refuse_sensitive(&self.context.engine, panel, path).await {
                return Ok(refusal);
            }
        }
        self.run(Command::ArchiveTest {
            panel,
            paths: params.paths,
        })
        .await
    }
}

#[cfg(test)]
#[path = "archives_tests.rs"]
mod tests;
