//! The cancellation token of a call (RC-080-50): a caller that can give up on a command - the
//! MCP server when its client cancels the request, the window's Cancel - runs it with a token
//! ([`Engine::dispatch_cancellable`], [`Engine::dispatch_mcp_cancellable`]), and a command's
//! long work - a comparison by content - stops at the token's next check with `cancelled`.
//! Without a token a command runs to its end, as before.

use tokio_util::sync::CancellationToken;

use crate::{Command, Engine, EngineError, Outcome};

tokio::task_local! {
    /// The token of the running call, when its caller gave one.
    static CALL: CancellationToken;
}

/// The token of the running call; outside a cancellable call one that is never cancelled.
pub(crate) fn call_token() -> CancellationToken {
    CALL.try_with(CancellationToken::clone).unwrap_or_default()
}

impl Engine {
    /// [`Engine::dispatch`], stopped where the command checks `cancel` once it is cancelled.
    ///
    /// # Errors
    ///
    /// As [`Engine::dispatch`]; `cancelled` when `cancel` stopped it.
    pub async fn dispatch_cancellable(
        &self,
        command: Command,
        cancel: CancellationToken,
    ) -> Result<Outcome, EngineError> {
        CALL.scope(cancel, self.dispatch(command)).await
    }

    /// [`Engine::dispatch_mcp`], stopped where the command checks `cancel` once it is cancelled:
    /// the MCP server passes the token of the client's request.
    ///
    /// # Errors
    ///
    /// As [`Engine::dispatch_mcp`]; `cancelled` when `cancel` stopped it.
    pub async fn dispatch_mcp_cancellable(
        &self,
        command: Command,
        cancel: CancellationToken,
    ) -> Result<Outcome, EngineError> {
        CALL.scope(cancel, self.dispatch_mcp(command)).await
    }
}
