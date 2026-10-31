//! The tools, one module per tool group; each is a thin wrapper over engine commands.

mod agent;
mod app;
mod archive_ops;
mod archives;
mod certificates;
mod checksum;
mod classify;
mod compare;
pub(crate) mod connections;
mod connections_save;
mod credentials;
mod database;
mod diagnostics;
mod drives;
mod favourites;
mod file_list;
mod files_read;
mod files_search;
mod flat;
mod hostkeys;
mod hotlist;
mod login_scripts;
mod multi_rename;
mod ops;
mod panel_filter;
mod panels;
mod plugins;
mod properties;
mod queue;
mod rdm_import;
mod remote;
mod secret_args;
mod secret_sources;
mod sessions;
mod settings;
mod ssh_exec;
mod ssh_import;
mod tabs;
mod tc_import;
mod terminal;
mod theme;
mod update;
mod vault;
mod wsl;

use std::future::Future;

use rc_engine::{Command, Outcome};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
};

tokio::task_local! {
    /// The running tool call's cancellation: the client's `notifications/cancelled` for it.
    static CALL_CANCEL: CancellationToken;
}

/// Runs `call`, one tool call, with `cancel` as the token its engine commands stop on
/// ([`call_cancel`]).
pub(crate) async fn with_call_cancel<F: Future>(cancel: CancellationToken, call: F) -> F::Output {
    CALL_CANCEL.scope(cancel, call).await
}

/// The running tool call's cancellation; outside one a token nobody cancels.
pub(crate) fn call_cancel() -> CancellationToken {
    CALL_CANCEL
        .try_with(CancellationToken::clone)
        .unwrap_or_default()
}

impl RcMcpServer {
    /// `command` dispatched as MCP, its outcome or error as the tool's answer (RC-080-27). A
    /// client that cancels the call stops the command (compare, synchronize: RC-080-50).
    pub(crate) async fn dispatch_mcp(&self, command: Command) -> ToolResult {
        self.dispatch_as(command, std::convert::identity).await
    }

    /// [`Self::dispatch_mcp`] with the outcome mapped by `view` before it is answered.
    pub(crate) async fn dispatch_as<T: Serialize>(
        &self,
        command: Command,
        view: impl FnOnce(Outcome) -> T,
    ) -> ToolResult {
        let engine = &self.context.engine;
        respond(
            engine
                .dispatch_mcp_cancellable(command, call_cancel())
                .await
                .map(view),
        )
    }
}

#[cfg(test)]
pub(crate) use classify::class_of;
pub(crate) use classify::effective_class;
