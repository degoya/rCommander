//! The MCP audit log (`mcp_audit_log`, RC-080-66): one row per tool call the MCP gate decided -
//! the session, the tool, its class, the decision, the answer's code and the arguments, which
//! the caller masked; never an answer. Every text is cut to its column's bound here, at a
//! character boundary, so no caller can write past it. A retained table
//! ([`crate::RETAINED_TABLES`]): the shared sweep cuts it by rows and age. Read and cleared by the
//! settings dialog only; no MCP tool reads it.

use serde::Serialize;

use crate::{Db, Result};

/// The most bytes of a call's arguments a row keeps.
pub const MCP_AUDIT_ARGUMENTS_MAX_BYTES: usize = 2048;

/// The most bytes of a session or tool name a row keeps (an unknown tool's name is the agent's).
pub const MCP_AUDIT_NAME_MAX_BYTES: usize = 128;

/// The most bytes of a class or decision a row keeps.
const SHORT_MAX_BYTES: usize = 32;

/// The most rows one read returns.
pub const MCP_AUDIT_MAX_PAGE: u32 = 1_000;

/// The most sessions the filter lists.
pub const MCP_AUDIT_MAX_SESSIONS: u32 = 200;

/// What the gate writes for one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMcpAuditEntry {
    /// The MCP session (`http:<id>`, `instance:<hex>`).
    pub session: String,
    pub tool: String,
    /// The class the gate held the call to; `None` where it decided before (an unknown tool).
    pub class: Option<String>,
    /// `allowed`, `asked`, `confirmed`, `declined`, `denied`, `refused` or `unknown`.
    pub decision: String,
    /// `ok`, or the answer's error code.
    pub code: String,
    /// The arguments as JSON, masked by the caller.
    pub arguments: String,
}

/// One row as the settings dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct McpAuditEntry {
    pub id: i64,
    pub recorded_at: String,
    pub session: String,
    pub tool: String,
    pub class: Option<String>,
    pub decision: String,
    pub code: String,
    pub arguments: String,
}

/// `text` cut to at most `max` bytes at a character boundary.
#[must_use]
pub fn cut_to_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

impl Db {
    /// Records one call the gate decided, each text cut to its bound.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn record_mcp_call(&self, entry: &NewMcpAuditEntry) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query(
            "INSERT INTO mcp_audit_log (session, tool, class, decision, code, arguments) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(cut_to_bytes(&entry.session, MCP_AUDIT_NAME_MAX_BYTES))
        .bind(cut_to_bytes(&entry.tool, MCP_AUDIT_NAME_MAX_BYTES))
        .bind(
            entry
                .class
                .as_deref()
                .map(|class| cut_to_bytes(class, SHORT_MAX_BYTES)),
        )
        .bind(cut_to_bytes(&entry.decision, SHORT_MAX_BYTES))
        .bind(cut_to_bytes(&entry.code, MCP_AUDIT_NAME_MAX_BYTES))
        .bind(cut_to_bytes(
            &entry.arguments,
            MCP_AUDIT_ARGUMENTS_MAX_BYTES,
        ))
        .execute(&mut *writer)
        .await?;
        Ok(())
    }

    /// The latest `limit` calls (at most [`MCP_AUDIT_MAX_PAGE`]), newest first; of one session
    /// when `session` names it.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn mcp_audit_log(
        &self,
        session: Option<&str>,
        limit: u32,
    ) -> Result<Vec<McpAuditEntry>> {
        let limit = i64::from(limit.min(MCP_AUDIT_MAX_PAGE));
        let rows: Vec<McpAuditEntry> = match session {
            Some(session) => {
                sqlx::query_as(
                    "SELECT id, recorded_at, session, tool, class, decision, code, arguments \
                     FROM mcp_audit_log WHERE session = ? ORDER BY id DESC LIMIT ?",
                )
                .bind(session)
                .bind(limit)
                .fetch_all(self.readers())
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, recorded_at, session, tool, class, decision, code, arguments \
                     FROM mcp_audit_log ORDER BY id DESC LIMIT ?",
                )
                .bind(limit)
                .fetch_all(self.readers())
                .await?
            }
        };
        Ok(rows)
    }

    /// The sessions the log holds calls of, the one with the newest call first (at most
    /// [`MCP_AUDIT_MAX_SESSIONS`]): the choices of the dialog's filter.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn mcp_audit_sessions(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT session FROM mcp_audit_log GROUP BY session ORDER BY MAX(id) DESC LIMIT ?",
        )
        .bind(i64::from(MCP_AUDIT_MAX_SESSIONS))
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(|(session,)| session).collect())
    }

    /// Deletes every row; how many there were.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn clear_mcp_audit_log(&self) -> Result<u64> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM mcp_audit_log")
            .execute(&mut *writer)
            .await?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
#[path = "mcp_audit_log_tests.rs"]
mod tests;
