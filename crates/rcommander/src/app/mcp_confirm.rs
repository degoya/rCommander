//! The question of an MCP call in the window (RC-010-13).
//!
//! When the MCP policy asks about a tool call, the engine announces an [`McpQuestion`] and the
//! call waits. The window comes to the front and shows the oldest open question over everything,
//! the settings dialog included: the tool, its kind, its arguments (masked by the MCP server) and
//! the panel it acts on. Only a click allows: Allow, Allow for this session (write tools, and
//! executing tools on their target for some minutes) or Deny. Escape denies, and no other key or click reaches the window while a question is
//! open, so a key typed as the dialog appears cannot allow anything: Deny is the one stop of the
//! focus ring and has it as the question opens, so Enter and Space deny too (RC-080-16); the
//! Allow buttons are no stops. The answer is the `GuiOnly`
//! command `mcp.answer`, through the window's job queue; the engine's
//! [`EngineEvent::McpQuestionClosed`] closes a question answered, timed out or given up. The
//! status bar counts the questions waiting, and the calls allowed for a session - a write tool
//! for the session, an executing one on its terminal or connection for some minutes
//! (RC-010-27); a click there takes them all back (`mcp.revoke`).

mod view;

use iced::{
    Task,
    widget::{Id, operation},
    window,
};
use rc_engine::{Command, EngineEvent, McpAnswer};

pub(super) use self::view::{DENY_ID, body, grants, layer, status, title};
use super::{App, Effect, Message, escape::escape};

/// What the dialog's buttons say.
#[derive(Debug, Clone)]
pub enum McpConfirmMsg {
    Answer(u64, McpAnswer),
    /// The status bar: take back every call allowed for a session (RC-010-27).
    Revoke,
}

impl App {
    /// The questions' share of every message, before anything else sees it: the engine's
    /// question events, missed events, and while a question is open every key and click.
    /// `None` for a message the dialog leaves to the rest of the window.
    pub(super) fn mcp_confirm_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::McpQuestion { question }) => {
                if !self.mcp_questions.iter().any(|open| open.id == question.id) {
                    self.mcp_questions.push((**question).clone());
                }
                // Deny takes the focus ring: Enter denies (RC-080-16). A window of its own comes
                // to the front as it opens (`dialog_windows/question_window.rs`).
                let deny = operation::focus(Id::new(DENY_ID));
                let front = if self.dialog_windows.own {
                    Task::none()
                } else {
                    to_front()
                };
                Some(vec![Effect::Task(Task::batch([front, deny]))])
            }
            Message::Engine(EngineEvent::McpQuestionClosed { question }) => {
                self.mcp_questions.retain(|open| open.id != *question);
                Some(Vec::new())
            }
            Message::Engine(EngineEvent::McpGrantsChanged) => {
                self.mcp_grants = self.engine.mcp_grants();
                Some(Vec::new())
            }
            Message::EngineLagged => {
                self.mcp_questions = self.engine.mcp_questions();
                self.mcp_grants = self.engine.mcp_grants();
                None
            }
            _ if self.mcp_questions.is_empty() => None,
            // Tab and Shift+Tab walk the buttons (RC-080-16).
            Message::Key(chord) if super::field_walk::tab(chord).is_some() => {
                super::field_walk::walk_key(chord)
            }
            Message::Key(chord) | Message::KeyCaptured(chord) => {
                if *chord != escape() {
                    return Some(Vec::new());
                }
                let first = self.mcp_questions.first().map(|question| question.id);
                Some(first.map_or_else(Vec::new, |id| self.answer_mcp(id, McpAnswer::Deny)))
            }
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// A button of the dialog.
    pub(super) fn on_mcp_confirm(&mut self, message: McpConfirmMsg) -> Vec<Effect> {
        match message {
            McpConfirmMsg::Answer(id, answer) => self.answer_mcp(id, answer),
            McpConfirmMsg::Revoke => {
                vec![Effect::Dispatch(
                    self.active,
                    vec![Command::RevokeMcpGrants {}],
                )]
            }
        }
    }

    /// Sends the answer; the question leaves the dialog at once, so a second click cannot
    /// answer it again.
    fn answer_mcp(&mut self, id: u64, answer: McpAnswer) -> Vec<Effect> {
        let Some(at) = self.mcp_questions.iter().position(|open| open.id == id) else {
            return Vec::new();
        };
        if answer == McpAnswer::AllowForSession && !self.mcp_questions[at].offers_session() {
            return Vec::new();
        }
        self.mcp_questions.remove(at);
        let command = Command::AnswerMcpQuestion {
            question: id,
            answer,
        };
        vec![Effect::Dispatch(self.active, vec![command])]
    }
}

/// The window to the front, and its taskbar button flashing until the person looks: the main
/// window out of the taskbar - a dialog's window it owns comes back with it - then the keyboard
/// to the newest window, where the question is drawn (RC-080-15).
fn to_front() -> Task<Message> {
    let main = window::oldest().and_then(|id| window::minimize(id, false));
    let top = window::latest().and_then(|id| {
        Task::batch([
            window::gain_focus(id),
            window::request_user_attention(id, Some(window::UserAttention::Critical)),
        ])
    });
    main.chain(top)
}

#[cfg(test)]
#[path = "mcp_confirm_tests.rs"]
mod tests;
