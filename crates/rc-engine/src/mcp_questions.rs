//! The questions an MCP call asks the person in the window (RC-010-13).
//!
//! The MCP server's gate (`rc-mcp`) asks before a tool its policy marks `ask` runs. While a
//! window is attached ([`Engine::attach_window`]), the question goes there: [`Engine::ask_mcp`]
//! files an [`McpQuestion`], announces it as [`EngineEvent::McpQuestion`] and waits for the
//! person's [`McpAnswer`]. The window's dialog answers with the command `mcp.answer`, which is
//! `GuiOnly`: no MCP tool can answer for the person. No answer in time is a no
//! ([`McpVerdict::TimedOut`]), and a call its client gives up takes its question back; either way
//! [`EngineEvent::McpQuestionClosed`] closes the dialog. "Allow for this session" is offered for
//! write tools, and for an executing tool on the target its call names (RC-010-27,
//! `mcp_grants.rs`), never for a destructive one.
//!
//! Without a window [`Engine::ask_mcp`] answers [`McpVerdict::NoWindow`] at once: what then
//! happens is the gate's decision (headless, nobody can be asked).

use std::{
    collections::VecDeque,
    fmt::Write as _,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{CommandClass, Engine, EngineError, EngineEvent, Outcome, PanelId};

#[path = "mcp_grants.rs"]
mod grants;
pub use grants::{MAX_MCP_GRANTS, McpGrant};

#[path = "mcp_pins.rs"]
mod pins;
pub(crate) use pins::check as check_mcp_pin;
pub use pins::{McpFrozenPanel, McpPin, with_mcp_pins};

/// How many questions wait at most; another one is refused at once ([`McpVerdict::Busy`]).
pub const MAX_MCP_QUESTIONS: usize = 8;

/// How many selected names a question shows of the panel it acts on.
const MAX_SHOWN_ENTRIES: usize = 20;

/// How many characters of one value a question shows; the rest is counted, not shown.
pub const MAX_SHOWN_CHARS: usize = 1000;

/// A tool call waiting for the person's decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpQuestion {
    /// The id the answer names.
    pub id: u64,
    /// The MCP tool.
    pub tool: String,
    /// The class the policy asked about, after the call's arguments raised it (an overwrite).
    pub class: CommandClass,
    /// The call's arguments as the person reads them: masked, clipped, the code left out.
    pub arguments: Vec<McpArgument>,
    /// The panel the call acts on as it stood when asked: what a call without sources takes.
    pub panel: McpPanelContext,
    /// What "allow for this session" grants, where the gate offers it.
    #[serde(default)]
    pub session: Option<McpSessionOffer>,
}

impl McpQuestion {
    /// Whether "allow for this session" may answer it.
    #[must_use]
    pub fn offers_session(&self) -> bool {
        self.session.is_some()
    }
}

/// What "allow for this session" grants (RC-010-27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpSessionOffer {
    /// A write tool, until the session ends.
    Session,
    /// An executing tool on the target its call names, for this many minutes.
    Minutes(u32),
}

/// One argument of a call, ready to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpArgument {
    pub name: String,
    pub value: String,
}

/// A panel as a question shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPanelContext {
    pub panel: PanelId,
    /// Its directory.
    pub path: String,
    /// The selected names, or else the entry under the cursor; at most [`MAX_SHOWN_ENTRIES`].
    pub entries: Vec<String>,
    /// Whether `entries` is the selection (rather than the cursor's entry).
    pub selection: bool,
    /// How many selected names are not in `entries`.
    pub more: usize,
}

/// What the gate asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAsk {
    pub tool: String,
    pub class: CommandClass,
    pub arguments: Vec<McpArgument>,
    /// The panel the call names; the active one when `None`.
    pub panel: Option<PanelId>,
    /// The call acts on the entry under the cursor whatever is selected (`file_open` without a
    /// path): the question names that entry, not the selection (RC-080-12).
    pub cursor: bool,
    /// What "allow for this session" would grant; `None` offers no such answer. A write tool
    /// only gets [`McpSessionOffer::Session`], a destructive one nothing (`ask_mcp`).
    pub session: Option<McpSessionOffer>,
}

/// The person's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAnswer {
    /// Run this call.
    Allow,
    /// Run this call and further ones of the tool in the same MCP session, as
    /// [`McpQuestion::session`] says.
    AllowForSession,
    /// Do not run it.
    Deny,
}

/// How a question ended, for the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpVerdict {
    Allowed,
    AllowedForSession,
    Denied,
    /// Nobody answered in time.
    TimedOut,
    /// No window is attached to ask in.
    NoWindow,
    /// [`MAX_MCP_QUESTIONS`] wait already.
    Busy,
}

/// The open questions, each with the channel its call waits on.
#[derive(Default)]
pub(crate) struct McpDesk {
    window: AtomicBool,
    inner: Mutex<Desk>,
}

#[derive(Default)]
struct Desk {
    next_id: u64,
    open: VecDeque<Open>,
    grants: grants::Grants,
}

struct Open {
    question: McpQuestion,
    reply: oneshot::Sender<McpAnswer>,
}

impl McpDesk {
    fn lock(&self) -> MutexGuard<'_, Desk> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes the question `id` off the desk; whether it was still open.
    fn take_back(&self, id: u64) -> bool {
        let mut desk = self.lock();
        let before = desk.open.len();
        desk.open.retain(|open| open.question.id != id);
        desk.open.len() != before
    }
}

/// Takes a question back when the call waiting on it ends without an answer: timed out, or
/// dropped because its client gave it up.
struct TakeBack<'a> {
    engine: &'a Engine,
    id: u64,
}

impl Drop for TakeBack<'_> {
    fn drop(&mut self) {
        if self.engine.inner.mcp_questions.take_back(self.id) {
            self.engine
                .publish(EngineEvent::McpQuestionClosed { question: self.id });
        }
    }
}

impl Engine {
    /// A window is there to answer MCP questions; set once, when the window starts.
    pub fn attach_window(&self) {
        self.inner
            .mcp_questions
            .window
            .store(true, Ordering::Release);
    }

    /// Whether a window answers MCP questions.
    #[must_use]
    pub fn has_window(&self) -> bool {
        self.inner.mcp_questions.window.load(Ordering::Acquire)
    }

    /// The questions waiting for the person, oldest first: what a window opened later shows.
    #[must_use]
    pub fn mcp_questions(&self) -> Vec<McpQuestion> {
        let desk = self.inner.mcp_questions.lock();
        desk.open.iter().map(|open| open.question.clone()).collect()
    }

    /// Asks the person in the window about `ask` and waits up to `within` for the answer; the
    /// panel as it stands now.
    pub async fn ask_mcp(&self, ask: McpAsk, within: Duration) -> McpVerdict {
        let panel = self.mcp_panel_context(ask.panel, ask.cursor);
        self.ask_mcp_about(ask, panel, within).await
    }

    /// [`Engine::ask_mcp`] showing `panel`, as the gate froze it before asking
    /// ([`Engine::freeze_mcp_panel`], RC-080-34); `ask.panel` and `ask.cursor` are not read
    /// again.
    pub async fn ask_mcp_about(
        &self,
        ask: McpAsk,
        panel: McpPanelContext,
        within: Duration,
    ) -> McpVerdict {
        if !self.has_window() {
            return McpVerdict::NoWindow;
        }
        let (reply, answer) = oneshot::channel();
        let question = {
            let mut desk = self.inner.mcp_questions.lock();
            if desk.open.len() >= MAX_MCP_QUESTIONS {
                return McpVerdict::Busy;
            }
            desk.next_id += 1;
            let question = McpQuestion {
                id: desk.next_id,
                tool: ask.tool,
                class: ask.class,
                arguments: ask.arguments,
                panel,
                session: offer(ask.class, ask.session),
            };
            desk.open.push_back(Open {
                question: question.clone(),
                reply,
            });
            question
        };
        let _take_back = TakeBack {
            engine: self,
            id: question.id,
        };
        self.publish(EngineEvent::McpQuestion {
            question: Box::new(question),
        });
        match tokio::time::timeout(within, answer).await {
            Ok(Ok(McpAnswer::Allow)) => McpVerdict::Allowed,
            Ok(Ok(McpAnswer::AllowForSession)) => McpVerdict::AllowedForSession,
            // A dropped sender is a desk that went away: no.
            Ok(Ok(McpAnswer::Deny) | Err(_)) => McpVerdict::Denied,
            Err(_) => McpVerdict::TimedOut,
        }
    }

    /// `mcp.answer`: the person's answer to an open question; the call waiting on it goes on.
    pub(crate) fn answer_mcp_question(
        &self,
        id: u64,
        answer: McpAnswer,
    ) -> Result<Outcome, EngineError> {
        let open = {
            let mut desk = self.inner.mcp_questions.lock();
            let at = desk
                .open
                .iter()
                .position(|open| open.question.id == id)
                .ok_or(EngineError::McpQuestionNotFound { question: id })?;
            if answer == McpAnswer::AllowForSession && !desk.open[at].question.offers_session() {
                return Err(EngineError::McpAnswerNotOffered { question: id });
            }
            desk.open
                .remove(at)
                .ok_or(EngineError::McpQuestionNotFound { question: id })?
        };
        // The call may have ended a moment ago; then the answer reaches nobody.
        let _ = open.reply.send(answer);
        self.publish(EngineEvent::McpQuestionClosed { question: id });
        Ok(Outcome::McpAnswered {
            question: id,
            answer,
        })
    }
}

/// The offer a question makes: a destructive tool gets none, a write tool none but the whole
/// session, an executing one none but its minutes - whatever the gate asked for.
fn offer(class: CommandClass, asked: Option<McpSessionOffer>) -> Option<McpSessionOffer> {
    match (class, asked?) {
        (CommandClass::Write, McpSessionOffer::Session) => Some(McpSessionOffer::Session),
        (CommandClass::Exec, McpSessionOffer::Minutes(minutes)) if minutes > 0 => {
            Some(McpSessionOffer::Minutes(minutes))
        }
        _ => None,
    }
}

/// `text` clipped to [`MAX_SHOWN_CHARS`], with control, zero-width and direction characters
/// spelled out (`\r`, `\u{202e}`): the person sees what the call carries, not what it shows.
#[must_use]
pub fn readable(text: &str) -> String {
    let mut shown = String::with_capacity(text.len().min(MAX_SHOWN_CHARS + 32));
    let mut chars = text.chars();
    for c in chars.by_ref().take(MAX_SHOWN_CHARS) {
        if matches!(c, '\r' | '\n' | '\t') {
            shown.extend(c.escape_default());
        } else if rc_rename::invisible(c) {
            shown.extend(c.escape_unicode());
        } else {
            shown.push(c);
        }
    }
    let rest = chars.count();
    if rest > 0 {
        let _ = write!(shown, " ... (+{rest} characters)");
    }
    shown
}

#[cfg(test)]
#[path = "mcp_questions_tests.rs"]
mod tests;
