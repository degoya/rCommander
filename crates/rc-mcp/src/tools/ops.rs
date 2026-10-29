//! The `ops` tool group: thin wrappers over the `ops.*` commands, the jobs of the operations
//! queue.
//!
//! An MCP caller cannot answer a dialog, so its jobs never ask: every conflict and every failing
//! file is decided by the call's presets (`on_conflict`, `on_error`, both `skip` by default) and
//! the failures are listed in the job. Overwriting loses data like deleting does: a copy or move
//! with any `overwrite*` `on_conflict` is gated as `destructive` (`Command::effective_class`,
//! RC-080-42), so it needs a confirmation code like `ops_delete`.
//!
//! The write guard (`guard_write.rs`, RC-080-09) judges every place a copy, move or mkdir writes
//! to before the job is queued: the target, what each source becomes there and every entry
//! below a local folder. The sources are fixed first, so the job takes exactly what was judged.

use rc_engine::{ConflictChoice, DeleteMode, ErrorPolicy, JobOptions, JobSnapshot, Outcome};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use super::{archive_ops::named, panels::PanelArg};
use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
};

/// The answer to every conflict of an MCP job.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConflictArg {
    /// Leave the target, do not copy the file.
    #[default]
    Skip,
    /// Copy under a free name beside the target: `name (2).ext`.
    Rename,
    /// Replace the target (needs a confirmation code).
    Overwrite,
    /// Replace the target only when it is older (needs a confirmation code).
    OverwriteOlder,
    /// Replace the target only when the source is larger (needs a confirmation code).
    OverwriteLarger,
    /// Replace the target only when the source is smaller (needs a confirmation code).
    OverwriteSmaller,
    /// Move the existing target aside to `name (2).ext`, then copy.
    RenameExisting,
    /// Stop the job at the first conflict.
    Cancel,
    /// Continue the <name>.rcpart an interrupted copy to or from a server left, from its length
    /// (when the source is unchanged and the part's last 64 KiB match; else it starts over);
    /// skip every other existing target.
    Resume,
}

impl From<ConflictArg> for ConflictChoice {
    fn from(arg: ConflictArg) -> Self {
        match arg {
            ConflictArg::Skip => Self::Skip,
            ConflictArg::Rename => Self::Rename,
            ConflictArg::Overwrite => Self::Overwrite,
            ConflictArg::OverwriteOlder => Self::OverwriteOlder,
            ConflictArg::OverwriteLarger => Self::OverwriteLarger,
            ConflictArg::OverwriteSmaller => Self::OverwriteSmaller,
            ConflictArg::RenameExisting => Self::RenameExisting,
            ConflictArg::Cancel => Self::Cancel,
            ConflictArg::Resume => Self::Resume,
        }
    }
}

/// What a failing file does in an MCP job.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorArg {
    /// List it in the job's failures and go on.
    #[default]
    Skip,
    /// Stop the job as failed.
    Cancel,
}

impl From<ErrorArg> for ErrorPolicy {
    fn from(arg: ErrorArg) -> Self {
        match arg {
            ErrorArg::Skip => Self::Skip,
            ErrorArg::Cancel => Self::Cancel,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct TransferParams {
    /// Entries: absolute, relative to the panel's directory, or on an SSH connection as
    /// sftp://<connection name>/<path>. Omitted or empty: the panel's selection, else the entry
    /// under its cursor.
    #[serde(default)]
    pub sources: Vec<String>,
    /// The directory to put them into (created when missing): absolute, relative to the other
    /// panel's directory, or sftp://<connection name>/<path>. Omitted: the other panel's
    /// directory.
    #[serde(default)]
    pub target: Option<String>,
    /// The panel the sources are in; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// When a target exists: skip (default), rename, rename_existing, overwrite,
    /// overwrite_older, overwrite_larger, overwrite_smaller, cancel, or resume (continue the
    /// <name>.rcpart an interrupted copy left).
    #[serde(default)]
    pub on_conflict: ConflictArg,
    /// When a file fails: skip (default) or cancel.
    #[serde(default)]
    pub on_error: ErrorArg,
    /// Compare BLAKE3 checksums of source and copy after each file.
    #[serde(default)]
    pub verify: bool,
    /// Keep time stamps and rights: the modification time, and the Unix mode where both sides
    /// have one. On when omitted.
    #[serde(default = "keep_by_default")]
    pub keep_attributes: bool,
}

impl TransferParams {
    /// The engine's request with `sources`, the ones the call names or the panel's.
    pub(crate) fn request(&self, sources: Vec<String>) -> rc_engine::TransferRequest {
        rc_engine::TransferRequest {
            panel: self.panel.map(Into::into),
            sources,
            target: self.target.clone(),
            options: self.options(),
        }
    }

    fn options(&self) -> JobOptions {
        JobOptions {
            on_conflict: Some(self.on_conflict.into()),
            on_error: self.on_error.into(),
            verify: self.verify,
            keep_attributes: self.keep_attributes,
            // The queue marks an MCP job itself (RC-080-42).
            ..JobOptions::default()
        }
    }
}

fn keep_by_default() -> bool {
    true
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct DeleteParams {
    /// Entries: absolute, relative to the panel's directory, or on a server as
    /// sftp://<connection name>/<path> or ftp://<connection name>/<path>. Omitted or empty: the
    /// panel's selection, else the entry under its cursor.
    #[serde(default)]
    pub sources: Vec<String>,
    /// The panel the sources are in; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// Delete for good instead of into the recycle bin.
    #[serde(default)]
    pub permanent: bool,
    /// When a file fails: skip (default) or cancel.
    #[serde(default)]
    pub on_error: ErrorArg,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MkdirParams {
    /// The directory to create (with missing parents): absolute, relative to the panel's
    /// directory, or on a server as sftp://<connection name>/<path> or ftp://...
    pub path: String,
    /// The panel a relative path starts from; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct JobParams {
    /// The job's id, as ops_copy and the others answered it.
    pub job: u64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// Only this job; every job when omitted.
    #[serde(default)]
    pub job: Option<u64>,
}

/// A queue answer as tools give it.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Job(Box<JobSnapshot>),
    Jobs {
        jobs: Vec<JobSnapshot>,
    },
    /// Boxed: an outcome is far larger than a job list.
    Other(Box<Outcome>),
}

fn view(outcome: Outcome) -> View {
    match outcome {
        Outcome::Job(job) => View::Job(job),
        Outcome::Jobs { jobs } => View::Jobs { jobs },
        other => View::Other(Box::new(other)),
    }
}

impl RcMcpServer {
    async fn queue(&self, command: rc_engine::Command) -> ToolResult {
        self.dispatch_as(command, view).await
    }

    /// `ops_copy` and `ops_move`: queued unless the write guard refuses a place they write to.
    /// The places are located as the engine locates them - a server's `sftp://` or `ftp://`
    /// location included - against the panels' folders pinned for the call, so the job writes
    /// exactly where the guard judged (RC-080-42).
    async fn transfer(&self, params: TransferParams, moving: bool) -> ToolResult {
        let engine = &self.context.engine;
        engine
            .pin_mcp_panels(async {
                // The panel fixed once: the engine takes the same one, not the active one then.
                let id = params
                    .panel
                    .map_or_else(|| engine.active_panel(), Into::into);
                let panel = Some(id);
                let sources = named(engine, panel, params.sources.clone());
                let other = id.other();
                let target = match params.target.as_deref() {
                    Some(target) => match engine.locate_target(Some(other), target).await {
                        Ok(target) => target,
                        Err(error) => return respond::<View>(Err(error)),
                    },
                    None => engine.mcp_folder(other),
                };
                let guard = self.write_guard();
                if let Some(refusal) = guard.refuse(&target).await {
                    return Ok(refusal);
                }
                for source in &sources {
                    let from = match engine.locate_target(panel, source).await {
                        Ok(from) => from,
                        Err(error) => return respond::<View>(Err(error)),
                    };
                    let Some(name) = from.path.file_name() else {
                        continue;
                    };
                    if let Some(refusal) = guard.refuse_copy(&from, &target.join(name)).await {
                        return Ok(refusal);
                    }
                }
                let mut request = params.request(sources);
                request.panel = panel;
                self.queue(if moving {
                    rc_engine::Command::OpsMove(request)
                } else {
                    rc_engine::Command::OpsCopy(request)
                })
                .await
            })
            .await
    }
}

#[tool_router(router = ops_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Copy files and directories into a directory, as a background job of the operations queue (the same queue the window shows). Directories are copied with their contents and merged into existing ones. Answers at once with the queued job (id, state, progress); with several sources and the setting queue.one_job_per_item on (the default) with one job per source instead (jobs, in the order given, sharing a batch number) - each listed, paused, resumed and cancelled on its own, a cancel or on_error cancel of one stopping the others too; follow them with ops_list. Existing targets: on_conflict skip (default), rename (the new file to name (2)), rename_existing (the old file aside), overwrite, overwrite_older, overwrite_larger, overwrite_smaller or cancel - every overwrite needs a confirmation code. Failing files are skipped and listed in failures (on_error cancel stops instead). verify compares BLAKE3 checksums; keep_attributes (default true) keeps modification times and, between Unix systems, permissions. Sources and target may be on an SSH server as sftp://<connection name>/<path> (logs in with what is stored); a file is written there under <name>.rcpart and renamed when complete, and at most two jobs per server run at once (the others wait, state queued). A broken connection is retried (transfers.retry_count, growing waits) and the file continues where its part ends; with the retries spent the job pauses with resumable=true for ops_resume. A <name>.rcpart an earlier copy left is a conflict: on_conflict resume continues it. Between two servers (sftp:// or ftp:// on both sides) the file streams through rCommander with no local copy; failures[].target names the target when it was the target that failed. Sources that are or hold keys or credential files are refused with sensitive_path before a job is queued. A target in rCommander's data folder, an SSH folder, an autostart folder or a shell's start-up file - also below it, for a copied folder - is refused with protected_target: a person copies there in rCommander. Errors: no_sources, same_location, target_inside_source, backend_unavailable, sensitive_path, protected_target, sftp_connection_not_found, sftp_connection_ambiguous, vault_locked, host_key_unknown, sftp_* codes; panel_changed when an asked call finds its panel or the target panel in another folder than the question showed."
    )]
    pub(crate) async fn ops_copy(
        &self,
        Parameters(params): Parameters<TransferParams>,
    ) -> ToolResult {
        self.transfer(params, false).await
    }

    #[tool(
        description = "Move files and directories into a directory, as a background job: renamed in place on the same drive, otherwise copied and each source removed once its copy is complete and its size checked on the target (also to and from sftp://<connection name>/<path>). Parameters, answer and refusals (sensitive_path, protected_target) as ops_copy; overwriting needs a confirmation code."
    )]
    pub(crate) async fn ops_move(
        &self,
        Parameters(params): Parameters<TransferParams>,
    ) -> ToolResult {
        self.transfer(params, true).await
    }

    #[tool(
        description = "Delete files and directories with their contents, as a background job: into the recycle bin by default, permanently with permanent=true. Always needs a confirmation code. Where there is no recycle bin (servers, archives, network shares) the answer is trash_unavailable; ask the person before deleting permanently. Answers with the queued job, or one job per source as ops_copy does (queue.one_job_per_item); failing files are listed in its failures. Without sources the question shows the selection (else the entry under the cursor) and exactly those are deleted, whatever is selected by the time the person allows it; panel_changed when the panel shows another folder by then."
    )]
    pub(crate) async fn ops_delete(
        &self,
        Parameters(params): Parameters<DeleteParams>,
    ) -> ToolResult {
        self.queue(rc_engine::Command::OpsDelete(rc_engine::DeleteRequest {
            panel: params.panel.map(Into::into),
            sources: params.sources,
            mode: if params.permanent {
                DeleteMode::Permanent
            } else {
                DeleteMode::Trash
            },
            on_error: params.on_error.into(),
            // An agent's delete is a job of the queue, as its copies are.
            queue: Some(rc_engine::DEFAULT_LIST),
        }))
        .await
    }

    #[tool(
        description = "Create a directory and its missing parents, as a job of the queue. An existing directory is listed as a failure (already_exists). One in rCommander's data folder, an SSH folder or an autostart folder is refused with protected_target."
    )]
    pub(crate) async fn ops_mkdir(
        &self,
        Parameters(params): Parameters<MkdirParams>,
    ) -> ToolResult {
        let engine = &self.context.engine;
        engine
            .pin_mcp_panels(async {
                let panel = Some(
                    params
                        .panel
                        .map_or_else(|| engine.active_panel(), Into::into),
                );
                // Where the command creates it: also a server's location (`sftp://`, `ftp://`),
                // against the folder pinned for the call (RC-080-42).
                let at = match engine.locate_target(panel, &params.path).await {
                    Ok(at) => at,
                    Err(error) => return respond::<View>(Err(error)),
                };
                if let Some(refusal) = self.write_guard().refuse(&at).await {
                    return Ok(refusal);
                }
                self.queue(rc_engine::Command::OpsMkdir {
                    panel,
                    path: params.path,
                })
                .await
            })
            .await
    }

    #[tool(
        description = "The jobs of the operations queue, oldest first, or one job: kind with sources and target, state (queued, counting, running, paused, waiting, completed, cancelled, failed), progress (files and bytes total and done, current file with file_bytes_done and file_bytes_total - its own bytes, from 0 at each file -, bytes_per_second, eta_seconds), skipped files and failures with their codes. resumable=true marks a paused transfer that continues where it stopped: one the last run of the app left in the middle (put back paused at the start) or one whose connection retries were spent; ops_resume continues it, ops_cancel discards it with its part file. Recently finished jobs stay listed."
    )]
    pub(crate) async fn ops_list(&self, Parameters(params): Parameters<ListParams>) -> ToolResult {
        self.queue(rc_engine::Command::OpsList { job: params.job })
            .await
    }

    #[tool(
        description = "Pause a job between two buffers of its current file; ops_resume continues it. Errors: job_not_found, job_finished."
    )]
    pub(crate) async fn ops_pause(&self, Parameters(params): Parameters<JobParams>) -> ToolResult {
        self.queue(rc_engine::Command::OpsPause { job: params.job })
            .await
    }

    #[tool(
        description = "Resume a paused job, also a resumable transfer from ops_list: it reconnects to its server and continues each file from its <name>.rcpart where the source is unchanged. Errors: job_not_found, job_finished."
    )]
    pub(crate) async fn ops_resume(&self, Parameters(params): Parameters<JobParams>) -> ToolResult {
        self.queue(rc_engine::Command::OpsResume { job: params.job })
            .await
    }

    #[tool(
        description = "Cancel a job, also a paused one. The file being written is removed; files already complete stay. Errors: job_not_found, job_finished."
    )]
    pub(crate) async fn ops_cancel(&self, Parameters(params): Parameters<JobParams>) -> ToolResult {
        self.queue(rc_engine::Command::OpsCancel { job: params.job })
            .await
    }
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
