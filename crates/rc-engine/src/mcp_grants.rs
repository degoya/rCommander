//! What a person allowed an MCP session for the rest of it (RC-010-27).
//!
//! "Allow for this session" on a write tool holds for that tool until the session ends. On an
//! executing tool it is offered only where the call names its target - the terminal typed
//! into, the connection a command runs on - and holds for that tool on that target only, for
//! the minutes of the setting `mcp.ask.exec_session_minutes`; another terminal or server asks
//! again. A destructive tool never gets one. The grants are kept here rather than in the MCP
//! server so the window can show them and take them all back (`mcp.revoke`, the status bar);
//! [`EngineEvent::McpGrantsChanged`] tells it when they change.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError, EngineEvent, Outcome};

/// How many grants are kept; the oldest goes first.
pub const MAX_MCP_GRANTS: usize = 256;

/// One grant as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpGrant {
    /// The MCP tool.
    pub tool: String,
    /// What it is bound to (`terminal=3`, `connection_id=7`); `None` for a write tool's.
    pub target: Option<String>,
}

struct Grant {
    session: String,
    grant: McpGrant,
    /// When it runs out; `None` holds until the session ends.
    until: Option<Instant>,
}

/// The grants of every MCP session, oldest first. Time is a parameter so the expiry edge can
/// be tested.
#[derive(Default)]
pub(crate) struct Grants(VecDeque<Grant>);

impl Grants {
    /// Whether `session` may call `tool` on `target` without a question at `now`.
    pub(crate) fn holds(
        &self,
        session: &str,
        tool: &str,
        target: Option<&str>,
        now: Instant,
    ) -> bool {
        self.0.iter().any(|held| {
            held.session == session
                && held.grant.tool == tool
                && held.grant.target.as_deref() == target
                && held.lasts(now)
        })
    }

    /// Grants `tool` on `target` to `session` until `until`; a grant of the same is replaced,
    /// so each is kept once, with its newest time.
    pub(crate) fn grant(
        &mut self,
        session: &str,
        tool: &str,
        target: Option<String>,
        until: Option<Instant>,
    ) {
        self.0.retain(|held| {
            held.session != session || held.grant.tool != tool || held.grant.target != target
        });
        if self.0.len() >= MAX_MCP_GRANTS {
            self.0.pop_front();
        }
        self.0.push_back(Grant {
            session: session.to_owned(),
            grant: McpGrant {
                tool: tool.to_owned(),
                target,
            },
            until,
        });
    }

    /// Drops the grants `now` has outlived; whether any went.
    pub(crate) fn prune(&mut self, now: Instant) -> bool {
        let before = self.0.len();
        self.0.retain(|held| held.lasts(now));
        self.0.len() != before
    }

    /// Drops every grant; how many there were.
    pub(crate) fn clear(&mut self) -> usize {
        let removed = self.0.len();
        self.0.clear();
        removed
    }

    pub(crate) fn live(&self, now: Instant) -> Vec<McpGrant> {
        self.0
            .iter()
            .filter(|held| held.lasts(now))
            .map(|held| held.grant.clone())
            .collect()
    }
}

impl Grant {
    fn lasts(&self, now: Instant) -> bool {
        self.until.is_none_or(|until| now < until)
    }
}

impl Engine {
    /// Whether a person allowed `session` to call `tool` on `target` without asking again; the
    /// grants that ran out meanwhile go, and the window is told.
    #[must_use]
    pub fn mcp_allowed_for_session(&self, session: &str, tool: &str, target: Option<&str>) -> bool {
        let now = Instant::now();
        let (held, pruned) = {
            let mut desk = self.inner.mcp_questions.lock();
            let pruned = desk.grants.prune(now);
            (desk.grants.holds(session, tool, target, now), pruned)
        };
        if pruned {
            self.publish(EngineEvent::McpGrantsChanged);
        }
        held
    }

    /// Remembers "allow for this session" for `tool` on `target`, for `within` or (`None`)
    /// until the session ends.
    pub fn allow_mcp_for_session(
        &self,
        session: &str,
        tool: &str,
        target: Option<String>,
        within: Option<Duration>,
    ) {
        let until = within.map(|within| Instant::now() + within);
        self.inner
            .mcp_questions
            .lock()
            .grants
            .grant(session, tool, target, until);
        self.publish(EngineEvent::McpGrantsChanged);
    }

    /// Drops the grants that ran out, telling the window when any did; the MCP server calls it
    /// when a grant's time is up.
    pub fn prune_mcp_grants(&self) {
        let pruned = self.inner.mcp_questions.lock().grants.prune(Instant::now());
        if pruned {
            self.publish(EngineEvent::McpGrantsChanged);
        }
    }

    /// The grants that hold now, oldest first; those that ran out go, and the window is told.
    #[must_use]
    pub fn mcp_grants(&self) -> Vec<McpGrant> {
        let now = Instant::now();
        let (live, pruned) = {
            let mut desk = self.inner.mcp_questions.lock();
            let pruned = desk.grants.prune(now);
            (desk.grants.live(now), pruned)
        };
        if pruned {
            self.publish(EngineEvent::McpGrantsChanged);
        }
        live
    }

    /// `mcp.revoke`: every grant of every session goes; the next call asks again.
    pub(crate) fn revoke_mcp_grants(&self) -> Result<Outcome, EngineError> {
        let removed = self.inner.mcp_questions.lock().grants.clear();
        self.publish(EngineEvent::McpGrantsChanged);
        Ok(Outcome::McpRevoked { removed })
    }
}

#[cfg(test)]
#[path = "mcp_grants_tests.rs"]
mod tests;
