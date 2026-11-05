//! What an MCP call's class was judged on, and the check that its run still finds it so
//! (RC-080-66), as `mcp_pins.rs` holds the panels a question showed.
//!
//! `Engine::judge_class` reads stored content to judge a class - a saved queue list with a job
//! that overwrites is destructive - and the call ran later on what it read again then:
//! `queue.save` replaces a list under its id, so a list judged harmless could be swapped for one
//! that overwrites in between. The gate now runs the call with what was judged held
//! ([`with_mcp_judged`]), and `queue.load` refuses a list that is no longer the one judged with
//! `queue_changed` ([`check_judged_queue`]).

use std::future::Future;

use crate::EngineError;

tokio::task_local! {
    /// What the class of the running MCP call was judged on.
    static JUDGED: McpJudged;
}

/// The stored content an MCP call's class was judged on; its run must find it unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpJudged {
    /// The saved queue list a `queue.load` was judged on: its id and its entries, `None` for a
    /// list that was not there.
    pub(crate) queue: Option<(i64, Option<Vec<String>>)>,
}

/// Runs `future`, one MCP tool call, with what its class was judged on held.
pub async fn with_mcp_judged<F: Future>(judged: McpJudged, future: F) -> F::Output {
    JUDGED.scope(judged, future).await
}

/// Refuses with `queue_changed` when the running MCP call's class was judged on another content
/// of saved list `saved` than `entries`, the one it loads now; `Ok` outside such a call.
pub(crate) fn check_judged_queue(saved: i64, entries: &[String]) -> Result<(), EngineError> {
    let changed = JUDGED
        .try_with(|judged| match &judged.queue {
            Some((id, judged)) if *id == saved => judged.as_deref() != Some(entries),
            _ => false,
        })
        .unwrap_or(false);
    if changed {
        return Err(EngineError::SavedQueueChanged { saved });
    }
    Ok(())
}

#[cfg(test)]
#[path = "mcp_judged_tests.rs"]
mod tests;
