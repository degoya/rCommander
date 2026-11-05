//! The MCP page's call log (RC-080-66): the latest tool calls of the agents, newest first, with
//! what the gate decided - read from `mcp_audit_log`, filtered by MCP session, cleared on
//! "Clear".
//!
//! Read and cleared straight from the database, as the token and the policy are: no engine
//! command reads the log, so nothing an agent can call does - an agent does not see what other
//! agents did. Read when the page is shown, when the filter changes and on "Refresh".

use iced::Task;
use rc_db::{Db, McpAuditEntry};

use super::{Page, SettingsMessage, wrap};
use crate::{
    app::{App, Effect, Message},
    i18n::{error_text, t},
};

/// How many calls the page lists at most.
pub const SHOWN: u32 = 200;

/// What the page keeps while the dialog is open.
#[derive(Debug, Default)]
pub struct McpLogPage {
    /// The session the list shows; `None` for every one.
    pub session: Option<String>,
    /// The sessions the log holds, the one with the newest call first.
    pub sessions: Vec<String>,
    /// The latest calls, newest first; `None` until read.
    pub entries: Option<Vec<McpAuditEntry>>,
    /// A read or the clearing runs; the buttons wait.
    pub busy: bool,
}

/// A choice of the filter: one session, or every one (`None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChoice(pub Option<String>);

impl std::fmt::Display for SessionChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(session) => formatter.write_str(session),
            None => formatter.write_str(t!("settings.mcp.log.all_sessions")),
        }
    }
}

/// What a read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpLogRead {
    /// The session it was read for.
    pub session: Option<String>,
    pub sessions: Vec<String>,
    pub entries: Vec<McpAuditEntry>,
}

#[derive(Debug, Clone)]
pub enum McpLogMsg {
    Filter(SessionChoice),
    Refresh,
    Clear,
    Read(Result<Box<McpLogRead>, &'static str>),
    Cleared(Result<u64, &'static str>),
}

pub fn page_message(message: McpLogMsg) -> Message {
    wrap(SettingsMessage::McpLog(message))
}

/// What the page asks the database for; its answer comes back as a [`McpLogMsg`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpLogCall {
    Read { session: Option<String> },
    Clear,
}

impl McpLogCall {
    pub async fn run(self, db: Db) -> McpLogMsg {
        match self {
            Self::Read { session } => McpLogMsg::Read(read(&db, session).await.map(Box::new)),
            Self::Clear => McpLogMsg::Cleared(db.clear_mcp_audit_log().await.map_err(|error| {
                tracing::warn!(%error, "the MCP audit log was not cleared");
                "database_error"
            })),
        }
    }
}

async fn read(db: &Db, session: Option<String>) -> Result<McpLogRead, &'static str> {
    let failed = |error: rc_db::DbError| {
        tracing::warn!(%error, "the MCP audit log does not read");
        "database_error"
    };
    let entries = db
        .mcp_audit_log(session.as_deref(), SHOWN)
        .await
        .map_err(failed)?;
    let sessions = db.mcp_audit_sessions().await.map_err(failed)?;
    Ok(McpLogRead {
        session,
        sessions,
        entries,
    })
}

/// What one message asks of the dialog: a database call, a note.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub call: Option<McpLogCall>,
    pub note: Option<(String, bool)>,
}

impl McpLogPage {
    /// The read of what the page shows now.
    pub fn read(&mut self) -> McpLogCall {
        self.busy = true;
        McpLogCall::Read {
            session: self.session.clone(),
        }
    }

    /// Applies `message`.
    pub fn update(&mut self, message: McpLogMsg) -> Step {
        let call = |call| Step {
            call: Some(call),
            note: None,
        };
        match message {
            McpLogMsg::Filter(SessionChoice(session)) => {
                self.session = session;
                return call(self.read());
            }
            McpLogMsg::Refresh if !self.busy => return call(self.read()),
            McpLogMsg::Clear if !self.busy => {
                self.busy = true;
                return call(McpLogCall::Clear);
            }
            McpLogMsg::Refresh | McpLogMsg::Clear => {}
            // An answer for another filter is dropped: the read of this one is on its way.
            McpLogMsg::Read(Ok(read)) if read.session != self.session => {}
            McpLogMsg::Read(Ok(read)) => {
                self.busy = false;
                // A session the log no longer holds shows every one again.
                if let Some(session) = &self.session
                    && !read.sessions.contains(session)
                    && read.entries.is_empty()
                {
                    self.session = None;
                    return call(self.read());
                }
                self.sessions = read.sessions;
                self.entries = Some(read.entries);
            }
            McpLogMsg::Read(Err(code)) | McpLogMsg::Cleared(Err(code)) => {
                self.busy = false;
                return Step {
                    call: None,
                    note: Some((error_text(code), true)),
                };
            }
            McpLogMsg::Cleared(Ok(count)) => {
                self.session = None;
                let note = t!("settings.mcp.log.cleared", count = count);
                return Step {
                    call: Some(self.read()),
                    note: Some((note, false)),
                };
            }
        }
        Step::default()
    }
}

impl App {
    /// The MCP page was turned to: its log is read (again).
    pub(super) fn mcp_log_shown(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        if dialog.page != Page::Mcp {
            return Vec::new();
        }
        let call = dialog.mcp_log.read();
        self.mcp_log_call(call)
    }

    /// A message of the log.
    pub(super) fn mcp_log_message(&mut self, message: McpLogMsg) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let step = dialog.mcp_log.update(message);
        if let Some(note) = step.note {
            dialog.note = Some(note);
        }
        match step.call {
            Some(call) => self.mcp_log_call(call),
            None => Vec::new(),
        }
    }

    fn mcp_log_call(&mut self, call: McpLogCall) -> Vec<Effect> {
        let Some(db) = self.db.clone() else {
            if let Some(dialog) = self.settings.as_mut() {
                dialog.mcp_log.busy = false;
            }
            return Vec::new();
        };
        vec![Effect::Task(Task::perform(call.run(db), page_message))]
    }
}

#[cfg(test)]
#[path = "mcp_log_tests.rs"]
mod tests;
