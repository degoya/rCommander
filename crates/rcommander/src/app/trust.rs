//! What the host-key (`hostkeys.rs`) and the certificate dialogs (`cert_trust.rs`) share: the
//! engine's open trust questions, one shown at a time over everything else, the step of its
//! dialog, the button the keyboard is on and the answer on its way ([`QuestionQueue`]), the keys
//! of such a dialog ([`QuestionQueue::key`]), and the drawn pieces (`trust/view.rs`).
//!
//! The keyboard starts on the safe button, the last of each dialog, and goes back to it whenever
//! another question is shown: Enter alone never trusts anything.

pub mod view;

use std::fmt;

use rc_engine::Chord;

use crate::i18n::error_text;

/// How many questions the window keeps at most, as many as the engine does.
const MAX_QUESTIONS: usize = 32;

/// A question of the engine's, as a trust dialog asks it.
pub trait Question {
    /// The steps of its dialog; the default is the first.
    type Step: Copy + Default + PartialEq + fmt::Debug;
    /// A button of its dialog.
    type Choice: Copy + PartialEq + fmt::Debug + 'static;

    fn id(&self) -> u64;

    /// The buttons of its dialog at `step`, the safe one last.
    fn buttons(&self, step: Self::Step) -> &'static [Self::Choice];
}

/// What a key asks of the shown dialog, besides moving between its buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed<C> {
    /// Escape: the safe way out of the current step.
    Escape,
    /// Enter: the button the keyboard is on.
    Chosen(C),
}

/// The open questions and the dialog over the first of them.
#[derive(Debug)]
pub struct QuestionQueue<Q: Question> {
    /// The questions no one has answered yet, oldest first; the dialog shows the first.
    questions: Vec<Q>,
    step: Q::Step,
    /// The button the keyboard is on, an index into [`QuestionQueue::buttons`].
    focus: usize,
    /// The question whose answer is on its way to the engine.
    answering: Option<u64>,
    /// Why the last answer failed.
    note: Option<String>,
}

impl<Q: Question> Default for QuestionQueue<Q> {
    fn default() -> Self {
        Self {
            questions: Vec::new(),
            step: Q::Step::default(),
            focus: 0,
            answering: None,
            note: None,
        }
    }
}

impl<Q: Question> QuestionQueue<Q> {
    /// The question the dialog shows.
    pub fn shown(&self) -> Option<&Q> {
        self.questions.first()
    }

    pub fn shown_id(&self) -> Option<u64> {
        self.shown().map(Q::id)
    }

    /// The buttons of the shown dialog, the safe one last.
    pub fn buttons(&self) -> &'static [Q::Choice] {
        match self.shown() {
            Some(question) => question.buttons(self.step),
            None => &[],
        }
    }

    pub fn step(&self) -> Q::Step {
        self.step
    }

    pub fn focused(&self) -> Option<Q::Choice> {
        self.buttons().get(self.focus).copied()
    }

    /// Whether the shown question's answer is on its way.
    pub fn busy(&self) -> bool {
        self.answering.is_some() && self.answering == self.shown_id()
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// How many questions wait behind the shown one.
    pub fn waiting(&self) -> usize {
        self.questions.len().saturating_sub(1)
    }

    /// Files a question the engine announced, unless it is here already.
    pub fn ask(&mut self, question: Q) {
        if self.questions.iter().any(|open| open.id() == question.id()) {
            return;
        }
        self.questions.push(question);
        if self.questions.len() > MAX_QUESTIONS {
            self.questions.remove(0);
            self.restart();
        } else if self.questions.len() == 1 {
            self.restart();
        }
    }

    /// Drops a question the engine closed or no longer has.
    pub fn close(&mut self, id: u64) {
        let shown = self.shown_id();
        self.questions.retain(|question| question.id() != id);
        if shown == Some(id) {
            self.restart();
        }
    }

    /// The questions as the engine has them, after missed events.
    pub fn resync(&mut self, open: Vec<Q>) {
        let shown = self.shown_id();
        self.questions
            .retain(|kept| open.iter().any(|question| question.id() == kept.id()));
        for question in open {
            self.ask(question);
        }
        if self.shown_id() != shown {
            self.restart();
        }
    }

    /// The shown dialog from its first step, the keyboard on the safe button.
    fn restart(&mut self) {
        self.go_to(Q::Step::default());
        self.note = None;
    }

    /// The shown dialog at `step`, the keyboard on its safe button.
    pub fn go_to(&mut self, step: Q::Step) {
        self.step = step;
        self.focus = self.buttons().len().saturating_sub(1);
    }

    fn move_focus(&mut self, forward: bool) {
        let count = self.buttons().len();
        if count > 0 {
            self.focus = if forward {
                (self.focus + 1) % count
            } else {
                (self.focus + count - 1) % count
            };
        }
    }

    /// A key while the dialog is shown: Tab and the arrows move between the buttons here;
    /// Escape and Enter come back for the dialog to act on. Nothing while the answer is on its
    /// way, nothing with Ctrl or Alt.
    pub fn key(&mut self, chord: &Chord) -> Option<Pressed<Q::Choice>> {
        if chord.ctrl || chord.alt || self.busy() {
            return None;
        }
        match (chord.key.as_str(), chord.shift) {
            ("Escape", false) => return Some(Pressed::Escape),
            ("Enter", false) => return self.focused().map(Pressed::Chosen),
            ("Tab" | "ArrowRight", false) => self.move_focus(true),
            ("Tab", true) | ("ArrowLeft", false) => self.move_focus(false),
            _ => {}
        }
        None
    }

    /// The shown question, when `choice` may answer it: a button its current step shows, and no
    /// answer on its way.
    pub fn answerable(&self, choice: Q::Choice) -> Option<&Q> {
        let shown = self.shown()?;
        (!self.busy() && self.buttons().contains(&choice)).then_some(shown)
    }

    /// The answer to question `id` goes to the engine.
    pub fn answering(&mut self, id: u64) {
        self.answering = Some(id);
        self.note = None;
    }

    /// The engine's answer to question `id`: the status bar's text when it took it (`Ok`); a
    /// question it no longer has (`gone`) goes as well. Another failure stays in the dialog with
    /// its reason while the question is shown. What the status bar says, if anything; the
    /// engine's close event may have come first.
    pub fn finished(
        &mut self,
        id: u64,
        result: Result<String, &'static str>,
        gone: &str,
    ) -> Option<String> {
        if self.answering == Some(id) {
            self.answering = None;
        }
        match result {
            Ok(info) => {
                self.close(id);
                Some(info)
            }
            Err(code) if code == gone => {
                self.close(id);
                Some(error_text(code))
            }
            Err(code) if self.shown_id() == Some(id) => {
                self.note = Some(error_text(code));
                None
            }
            Err(code) => Some(error_text(code)),
        }
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
