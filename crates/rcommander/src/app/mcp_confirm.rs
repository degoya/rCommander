//! The question of an MCP call in the window (RC-010-13).
//!
//! When the MCP policy asks about a tool call, the engine announces an [`McpQuestion`] and the
//! call waits. The window comes to the front and shows the oldest open question over everything,
//! the settings dialog included: the tool, its kind, its arguments (masked by the MCP server) and
//! the panel it acts on. The answers are Allow, Allow for this session (write tools, and
//! executing tools on their target for some minutes) and Deny. Escape denies, and no other key or
//! click reaches the window while a question is open. A key typed as the question appears answers
//! nothing (RC-110-80): Deny has the focus ring as the question comes on top, but Enter and Space
//! do nothing in its first second ([`KEYS_AFTER`]), and the Allow buttons are no stops of the ring
//! until then - only a click answers. A second later Tab reaches Allow and Enter or Space press
//! the button under the ring (RC-080-16), so the keyboard alone can allow a call. The answer is
//! the `GuiOnly` command `mcp.answer`, through the window's job queue; the engine's
//! [`EngineEvent::McpQuestionClosed`] closes a question answered, timed out or given up. The
//! status bar counts the questions waiting, and the calls allowed for a session - a write tool
//! for the session, an executing one on its terminal or connection for some minutes
//! (RC-010-27); a click there takes them all back (`mcp.revoke`).

mod view;

use std::time::Duration;

use iced::{
    Task,
    widget::{Id, operation},
    window,
};
use rc_engine::{Chord, Command, EngineEvent, McpAnswer};

pub(super) use self::view::{DENY_ID, body, grants, layer, status, title};
use super::{App, Effect, Message, escape::escape};

/// How long a question is on top before its keys work: Tab reaches Allow, Enter and Space press
/// the button under the ring (RC-110-80).
const KEYS_AFTER: Duration = Duration::from_secs(1);

/// What the dialog's buttons say.
#[derive(Debug, Clone)]
pub enum McpConfirmMsg {
    /// A click on a button.
    Answer(u64, McpAnswer),
    /// Enter or Space on a button of the ring: an answer only once the question's keys work.
    Key(u64, McpAnswer),
    /// [`KEYS_AFTER`] has passed since the question came on top: its keys work now.
    Armed(u64),
    /// The status bar: take back every call allowed for a session (RC-010-27).
    Revoke,
}

/// The question on top, and whether its keys work yet (RC-110-80).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Shown {
    id: u64,
    armed: bool,
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
                // A window of its own comes to the front as it opens
                // (`dialog_windows/question_window.rs`).
                let mut effects = Vec::new();
                if !self.dialog_windows.own {
                    effects.push(Effect::Task(to_front()));
                }
                effects.extend(self.mcp_on_top());
                Some(effects)
            }
            Message::Engine(EngineEvent::McpQuestionClosed { question }) => {
                self.mcp_questions.retain(|open| open.id != *question);
                Some(self.mcp_on_top())
            }
            Message::Engine(EngineEvent::McpGrantsChanged) => {
                self.mcp_grants = self.engine.mcp_grants();
                Some(Vec::new())
            }
            // A question that comes on top here gets its ring and its second with the next key
            // (`mcp_key`).
            Message::EngineLagged => {
                self.mcp_questions = self.engine.mcp_questions();
                self.mcp_grants = self.engine.mcp_grants();
                None
            }
            _ if self.mcp_questions.is_empty() => None,
            Message::Key(chord) => Some(self.mcp_key(chord, false)),
            Message::KeyCaptured(chord) => Some(self.mcp_key(chord, true)),
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// A button of the dialog.
    pub(super) fn on_mcp_confirm(&mut self, message: McpConfirmMsg) -> Vec<Effect> {
        match message {
            McpConfirmMsg::Answer(id, answer) => self.answer_mcp(id, answer),
            // Enter or Space in the question's first second is no answer (RC-110-80).
            McpConfirmMsg::Key(id, answer) => {
                if self.mcp_armed() && self.mcp_shown.is_some_and(|shown| shown.id == id) {
                    self.answer_mcp(id, answer)
                } else {
                    Vec::new()
                }
            }
            McpConfirmMsg::Armed(id) => {
                if let Some(shown) = self.mcp_shown.as_mut()
                    && shown.id == id
                {
                    shown.armed = true;
                }
                Vec::new()
            }
            McpConfirmMsg::Revoke => {
                vec![Effect::Dispatch(
                    self.active,
                    vec![Command::RevokeMcpGrants {}],
                )]
            }
        }
    }

    /// Whether the keys of the question on top work: [`KEYS_AFTER`] after it came on top. The
    /// view makes the Allow buttons stops of the ring then.
    pub(super) fn mcp_armed(&self) -> bool {
        let top = self.mcp_questions.first().map(|question| question.id);
        self.mcp_shown
            .is_some_and(|shown| shown.armed && Some(shown.id) == top)
    }

    /// A key while a question is open: Escape denies, Tab and Shift+Tab walk the buttons
    /// (RC-080-16); Enter and Space reach the button under the ring through its stop, so every
    /// other key does nothing here.
    fn mcp_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        if *chord == escape() {
            let first = self.mcp_questions.first().map(|question| question.id);
            return first.map_or_else(Vec::new, |id| self.answer_mcp(id, McpAnswer::Deny));
        }
        // A question on top that no event brought up - after missed events, or open as the
        // window started - gets the ring and its second now; the key itself does nothing.
        let on_top = self.mcp_on_top();
        if !on_top.is_empty() || captured {
            return on_top;
        }
        super::field_walk::walk_key(chord).unwrap_or_default()
    }

    /// The oldest open question as it comes on top: the ring to Deny, the safe answer
    /// (RC-080-16), and its keys working [`KEYS_AFTER`] later, so a key typed as it appears
    /// answers nothing - also when the next question follows an answered one. Nothing while the
    /// same question stays on top.
    fn mcp_on_top(&mut self) -> Vec<Effect> {
        let top = self.mcp_questions.first().map(|question| question.id);
        if self.mcp_shown.map(|shown| shown.id) == top {
            return Vec::new();
        }
        self.mcp_shown = top.map(|id| Shown { id, armed: false });
        let Some(id) = top else {
            return Vec::new();
        };
        let deny = operation::focus(Id::new(DENY_ID));
        let armed = Task::future(tokio::time::sleep(KEYS_AFTER))
            .map(move |()| Message::McpConfirm(McpConfirmMsg::Armed(id)));
        vec![Effect::Task(Task::batch([deny, armed]))]
    }

    /// Sends the answer; the question leaves the dialog at once, so a second click cannot
    /// answer it again, and the next one comes on top.
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
        let mut effects = vec![Effect::Dispatch(self.active, vec![command])];
        effects.extend(self.mcp_on_top());
        effects
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
