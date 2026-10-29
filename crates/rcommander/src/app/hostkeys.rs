//! The host-key dialogs (RC-050-07): the engine's [`HostKeyQuestion`]s, one at a time, over
//! everything else in the window.
//!
//! An unknown key opens the first-sighting (TOFU) dialog: server, key type, `SHA256:`
//! fingerprint and randomart, with "Connect once", "Trust and save" and "Cancel". A changed key
//! opens a different dialog: a loud warning with the stored and the presented fingerprint and only
//! "Replace stored key..." and "Cancel"; the replace action is a second, explicit step of its own
//! ("Replace the key", "Back"). No dialog offers a yes for a changed key, and the keyboard starts
//! on the safe button (Cancel, Back) - Enter alone never trusts anything.
//!
//! The answers are the engine's `GuiOnly` commands `hostkeys.answer` and `hostkeys.replace`; the
//! dialog only sends the choices its current step shows. While a question is open the dialog has
//! the keys - Tab and the arrows move between the buttons, Enter chooses, Escape cancels (or goes
//! back from the replace step) - and the clicks; nothing reaches the panels under it.

mod view;

use rc_engine::{
    Chord, Command, Engine, EngineEvent, HostKeyAnswer, HostKeyQuestion, HostKeyQuestionKind,
    Outcome,
};

use super::{
    App, Effect, Message,
    trust::{Pressed, Question, QuestionQueue},
};
use crate::i18n::t;

/// The error code of an answer to a question the engine no longer has.
const QUESTION_GONE: &str = "host_key_question_not_found";

/// A button of a host-key dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Unknown key: allow the next connection with this key, store nothing.
    ConnectOnce,
    /// Unknown key: store it as trusted.
    TrustAndSave,
    /// Do not connect.
    Cancel,
    /// Changed key: go to the replace step.
    Replace,
    /// The replace step: replace the stored key with the presented one.
    ConfirmReplace,
    /// The replace step: back to the warning.
    Back,
}

/// The step of the shown question's dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Step {
    /// The first-sighting dialog, or the warning about a changed key.
    #[default]
    Ask,
    /// The explicit replace action of a changed key.
    ConfirmReplace,
}

/// A question as the dialog shows it.
#[derive(Debug, Clone)]
pub struct Shown {
    pub question: HostKeyQuestion,
    /// The presented key's randomart (`ssh-keygen -lv`), drawn once.
    pub randomart: Option<String>,
}

impl Shown {
    fn new(question: HostKeyQuestion) -> Self {
        let randomart = question.offered.randomart();
        Self {
            question,
            randomart,
        }
    }
}

impl Question for Shown {
    type Step = Step;
    type Choice = Choice;

    fn id(&self) -> u64 {
        self.question.id
    }

    fn buttons(&self, step: Step) -> &'static [Choice] {
        match (&self.question.kind, step) {
            (HostKeyQuestionKind::Unknown, _) => {
                &[Choice::ConnectOnce, Choice::TrustAndSave, Choice::Cancel]
            }
            (HostKeyQuestionKind::Changed { .. }, Step::Ask) => &[Choice::Replace, Choice::Cancel],
            (HostKeyQuestionKind::Changed { .. }, Step::ConfirmReplace) => {
                &[Choice::ConfirmReplace, Choice::Back]
            }
        }
    }
}

/// The open questions and the dialog over the first of them.
pub type HostKeyDialogs = QuestionQueue<Shown>;

/// What a dialog's controls and the engine's answer say.
#[derive(Debug, Clone)]
pub enum HostKeyMsg {
    /// A button of the dialog of question `id`; a click on a dialog no longer shown is dropped.
    Choose(u64, Choice),
    /// The engine's answer to question `id` for `server` (`host:port`): the outcome, or the
    /// code of the failure.
    Finished {
        id: u64,
        server: String,
        result: Box<Result<Outcome, &'static str>>,
    },
}

/// An answer of the dialog for the engine.
pub struct HostKeyJob {
    question: u64,
    /// `host:port`, for the status bar: the question may be closed before the reply arrives.
    server: String,
    command: Command,
}

impl HostKeyJob {
    /// Runs the answer: off the window's thread in the app, inline in the tests.
    pub async fn run(self, engine: Engine) -> HostKeyMsg {
        let result = engine
            .dispatch(self.command)
            .await
            .map_err(|error| error.code());
        HostKeyMsg::Finished {
            id: self.question,
            server: self.server,
            result: Box::new(result),
        }
    }
}

impl App {
    /// The questions the engine files and closes, and - while one is open - the keys and clicks,
    /// which belong to its dialog. `None` for a message the dialogs leave to the rest.
    pub(super) fn host_key_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::HostKeyQuestion { question }) => {
                self.host_keys.ask(Shown::new(question.as_ref().clone()));
                Some(Vec::new())
            }
            Message::Engine(EngineEvent::HostKeyQuestionClosed { question }) => {
                self.host_keys.close(*question);
                Some(Vec::new())
            }
            Message::EngineLagged => {
                let open = self.engine.host_key_questions();
                self.host_keys
                    .resync(open.into_iter().map(Shown::new).collect());
                None
            }
            _ if self.host_keys.shown().is_none() => None,
            Message::Key(chord) => Some(self.host_key_key(chord)),
            // A key a terminal or a prompt under the dialog took runs nothing else either.
            Message::KeyCaptured(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// A message of the dialogs.
    pub(super) fn on_host_key(&mut self, message: HostKeyMsg) -> Vec<Effect> {
        match message {
            HostKeyMsg::Choose(id, choice) => {
                if self.host_keys.shown_id() == Some(id) {
                    return self.host_key_choice(choice);
                }
            }
            HostKeyMsg::Finished { id, server, result } => {
                self.host_key_finished(id, &server, *result);
            }
        }
        Vec::new()
    }

    /// A key while a dialog is open: Tab and the arrows move, Enter chooses, Escape cancels.
    fn host_key_key(&mut self, chord: &Chord) -> Vec<Effect> {
        match self.host_keys.key(chord) {
            Some(Pressed::Escape) => self.host_key_choice(match self.host_keys.step() {
                Step::Ask => Choice::Cancel,
                Step::ConfirmReplace => Choice::Back,
            }),
            Some(Pressed::Chosen(choice)) => self.host_key_choice(choice),
            None => Vec::new(),
        }
    }

    /// `choice` on the shown question - only a button its current step shows.
    fn host_key_choice(&mut self, choice: Choice) -> Vec<Effect> {
        let dialogs = &mut self.host_keys;
        let Some(shown) = dialogs.answerable(choice) else {
            return Vec::new();
        };
        let id = shown.question.id;
        let server = format!("{}:{}", shown.question.host, shown.question.port);
        let answer = |answer| Command::AnswerHostKey {
            question: id,
            answer,
        };
        let command = match choice {
            Choice::Replace => {
                dialogs.go_to(Step::ConfirmReplace);
                return Vec::new();
            }
            Choice::Back => {
                dialogs.go_to(Step::Ask);
                return Vec::new();
            }
            Choice::ConnectOnce => answer(HostKeyAnswer::ConnectOnce),
            Choice::TrustAndSave => answer(HostKeyAnswer::TrustAndSave),
            Choice::Cancel => answer(HostKeyAnswer::Cancel),
            Choice::ConfirmReplace => Command::ReplaceHostKey { question: id },
        };
        dialogs.answering(id);
        vec![Effect::HostKey(Box::new(HostKeyJob {
            question: id,
            server,
            command,
        }))]
    }

    /// The engine's answer: the question goes and the status bar says what happened, or the
    /// dialog stays with the reason. The engine's close event may have come first.
    fn host_key_finished(&mut self, id: u64, server: &str, result: Result<Outcome, &'static str>) {
        let result = result.map(|outcome| match outcome {
            Outcome::HostKeyAnswered {
                answer: HostKeyAnswer::ConnectOnce,
                ..
            } => t!("hostkeys.info_once", host = server),
            Outcome::HostKeyAnswered {
                answer: HostKeyAnswer::TrustAndSave,
                ..
            } => t!("hostkeys.info_saved", host = server),
            Outcome::HostKeyReplaced { .. } => t!("hostkeys.info_replaced", host = server),
            // A cancel, the only answer left.
            _ => t!("hostkeys.info_cancelled", host = server),
        });
        if let Some(info) = self.host_keys.finished(id, result, QUESTION_GONE) {
            self.info = Some(info);
        }
    }
}

#[cfg(test)]
#[path = "hostkeys_tests.rs"]
mod tests;
