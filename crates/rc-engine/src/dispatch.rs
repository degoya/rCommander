//! [`Engine::dispatch`]: every command to the method that runs it, and the MCP server's way in.
//!
//! The arms of the one `match` live with their registry group, `dispatch/arms/<group>.rs` for
//! `registry/<group>.rs` (RC-080-20); `dispatch_match!` gathers them in the order of
//! [`crate::command_groups!`], the order of `command.rs`.

mod arms;
mod cancel;
mod mcp;

pub(crate) use cancel::call_token;

use crate::{Command, Engine, EngineError, Outcome, read_guard::from_mcp};

/// Writes the body of [`Engine::dispatch`]: hands the arms gathered so far to the next group's
/// `arms!`, and after the last group writes the `match` over all of them. It is
/// exhaustive, so a [`Command`] variant without an arm does not compile.
macro_rules! dispatch_match {
    (($command:ident), $self:ident, [$($group:ident,)*]) => {
        dispatch_match!(($command, [$($group),*]), $self, {})
    };
    (($command:ident, []), $self:ident, { $($arms:tt)* }) => {
        match $command {
            $($arms)*
        }
    };
    (($command:ident, [$group:ident $(, $rest:ident)*]), $self:ident, { $($arms:tt)* }) => {
        arms::$group::arms! { dispatch_match, ($command, [$($rest),*]), $self, { $($arms)* } }
    };
}

impl Engine {
    /// Runs one command.
    ///
    /// # Errors
    ///
    /// The command's own failure, with a stable [`EngineError::code`].
    pub async fn dispatch(&self, command: Command) -> Result<Outcome, EngineError> {
        // The match's future holds the largest arm's state, and grows with every command; boxed,
        // it lives on the heap instead of inside every caller's future - in a debug build a
        // caller's stack ran out with it (RC-010-27).
        Box::pin(self.dispatch_unboxed(command)).await
    }

    async fn dispatch_unboxed(&self, command: Command) -> Result<Outcome, EngineError> {
        crate::command_groups!(dispatch_match, (command), self)
    }
}
