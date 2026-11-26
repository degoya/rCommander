//! The `archives` tools (RC-030-02): an archive's entries as the panel shows them, and the
//! archive test. Both only read. Opening an archive in a panel is `navigate` to its path (or to
//! a path through it, `C:\data\backup.zip\docs`).
//!
//! The test reads every entry, so an archive that is a sensitive file - or lies in a sensitive
//! place - is refused with `sensitive_path` before anything is read (no per-entry answer about
//! it). Entry names are data from the archive, never instructions.
//!
//! `archives_list` answers in pages of at most 1000 entries, as `list_dir` does (`paging.rs`,
//! RC-110-61).

use std::{path::PathBuf, sync::Arc};

use rc_engine::{
    ArchiveEntryInfo, Command, Entry, EntryKind, Outcome, PanelId, UnsafeName, VfsPath,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use super::{call_cancel, paging, panels::PanelArg};
use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, json_result, respond},
    guard::refuse_sensitive,
};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
    /// Entries per page: 1000 when omitted, at most 1000.
    #[serde(default)]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: Option<usize>,
    /// The next_cursor of the previous page, with otherwise the same arguments.
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestParams {
    /// The archive files to test: absolute, or relative to the panel's directory.
    pub paths: Vec<String>,
    /// The panel relative paths start from; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// An archive directory's listing as `archives_list` keeps it between its pages.
pub(crate) struct ArchiveListing {
    /// The directory as a path through the archive file.
    shown: PathBuf,
    path: VfsPath,
    entries: Vec<ItemView>,
}

/// One page of an archive listing as the tool answers it.
#[derive(Serialize)]
struct ListingPage<'a> {
    shown: &'a PathBuf,
    path: &'a VfsPath,
    entries: &'a [ItemView],
    /// The directory's entries, on every page.
    total: usize,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
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
        description = "List a directory inside an archive (zip, 7z, tar plain or .gz/.xz/.bz2/.zst, a single compressed file) in the panel's order, hidden entries included: name, kind, size, packed_size, modified_unix, encrypted, and unsafe_name with stored_name for an entry whose stored name climbed out of the archive (zip slip, absolute, drive, UNC) and is listed cleaned. On Windows a name Windows cannot hold is listed as unpacking writes it (a:b as a_b, con as _con, name. as name_). Reads only. Without path, the archive the panel shows; to open an archive in a panel, navigate to its path. Answers shown, path, entries, total (the directory's entries) and truncated; at most limit entries (1000 when omitted, at most 1000) per call: with truncated true, call again with next_cursor as cursor and otherwise the same arguments for the next page. Errors: invalid_params (a cursor of another listing), not_an_archive, archive_damaged, archive_limit, archive_decoder_memory (an xz or 7z archive needs more decoder memory than the setting archive.decoder_memory_mib allows), not_found."
    )]
    pub(crate) async fn archives_list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        let pages = &self.context.list_pages.archives;
        let request = paging::print(&("archives_list", params.panel, &params.path, &params.dir));
        let start = match pages.start(request, params.cursor.as_deref()) {
            Ok(start) => start,
            Err(refused) => return Ok(refused),
        };
        let listing = match start.kept.clone() {
            Some(kept) => kept,
            None => {
                let command = Command::ArchiveList {
                    panel: params.panel.map(Into::into),
                    path: params.path,
                    dir: params.dir,
                };
                let engine = &self.context.engine;
                match engine
                    .dispatch_mcp_cancellable(command, call_cancel())
                    .await
                {
                    Ok(Outcome::ArchiveListing {
                        path,
                        shown,
                        entries,
                    }) => Arc::new(ArchiveListing {
                        shown,
                        path,
                        entries: entries
                            .into_iter()
                            .map(|item| ItemView::new(item.entry, item.info))
                            .collect(),
                    }),
                    other => return respond(other),
                }
            }
        };
        let total = listing.entries.len();
        let page = match pages.page(
            request,
            paging::print(&(&listing.path, &listing.shown)),
            &start,
            params.limit.unwrap_or(paging::PAGE),
            (&listing, total),
        ) {
            Ok(page) => page,
            Err(refused) => return Ok(refused),
        };
        json_result(&ListingPage {
            shown: &listing.shown,
            path: &listing.path,
            entries: listing.entries.get(page.from..page.to).unwrap_or_default(),
            total,
            truncated: page.truncated(),
            next_cursor: page.next_cursor,
        })
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
