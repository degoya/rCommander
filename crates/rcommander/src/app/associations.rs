//! The internal associations' window (RC-100-91), as Total Commander's Files > "Internal
//! associations": File > "Internal associations...", the Programs page of the settings and the
//! palette open it (`file.associations`). On the left the rules in the order they are tried, with
//! a filter; Ctrl+Up and Ctrl+Down (or the buttons) move the rule at the cursor. On the right its
//! form: the mask, the description, the symbol and its commands - `open`, which Enter runs, and
//! named ones for the context menu -, each with the own commands' fields (RC-100-86). Save checks
//! every rule (`rc_engine::associations::check_rules`: a program needs `%1`, `%2` or another file
//! parameter) and stores them as the protected setting `files.associations`; Close drops what
//! was not saved.
//!
//! The window is always one of its own, as the settings are: the settings' Programs page opens
//! it beside themselves (`docs/design.md` reasoned in the job file). `known.rs` keeps the rules
//! and switches the context menu reads. The edits are `form.rs`, the keys `keys.rs`, the drawing
//! `view.rs`.

mod form;
mod keys;
pub mod known;
mod view;

use iced::{
    Task,
    widget::{Id, operation},
};
use rc_engine::{
    Associations,
    associations::{RuleError, check_rules, write_rules},
    settings::{ASSOCIATIONS_ALL_MATCHES_KEY, ASSOCIATIONS_ENABLED_KEY, FILE_ASSOCIATIONS_KEY},
};

pub use self::form::AssocWindow;
pub use self::view::{FILTER_FIELD, body};
use self::{
    form::RuleMsg,
    keys::{Focus, KeyAction},
};
use super::{App, Effect, Message, dialog_windows::DialogWindow, field_walk};
use crate::i18n::{error_text, t};

/// What the window's controls and the engine's answers say.
#[derive(Debug, Clone)]
pub enum AssocMsg {
    /// The settings' Programs page: the window opens, or comes to the front.
    Open,
    /// The rules and switches as the engine reads them, for the context menu.
    Known(Box<Associations>),
    Close,
    Filter(String),
    /// A rule clicked, by its place.
    Select(usize),
    /// The cursor moved by rows.
    Step(isize),
    New,
    Delete,
    /// The rule at the cursor one place up (`true`) or down.
    Move(bool),
    Edit(RuleMsg),
    Save,
    /// The store's answer: an error's code when it failed.
    Saved(Result<(), &'static str>),
    /// After Tab: the id of the stop of the focus ring that has the keyboard.
    Focused(Option<Id>),
}

/// Whether the setting `key` is one the context menu's rules read.
pub fn concerns(key: &str) -> bool {
    [
        FILE_ASSOCIATIONS_KEY,
        ASSOCIATIONS_ENABLED_KEY,
        ASSOCIATIONS_ALL_MATCHES_KEY,
    ]
    .contains(&key)
}

/// Why the rules do not fit, as the note says it: the rule, the command and the reason.
fn refusal(error: &RuleError) -> String {
    let why = match error.kind.code() {
        "association_mask" => t!("associations.error.mask"),
        "association_text" => t!("associations.error.text"),
        "association_verb_name" => t!("associations.error.verb_name"),
        "association_verb_twice" => t!("associations.error.verb_twice"),
        "association_too_many_verbs" => t!("associations.error.too_many_verbs"),
        "association_needs_file_parameter" => t!("associations.error.needs_file_parameter"),
        "association_command" => t!("associations.error.command"),
        "association_too_many" => t!("associations.error.too_many"),
        _ => t!("associations.error.too_large"),
    };
    match (error.rule, &error.verb) {
        (Some(rule), Some(verb)) => t!(
            "associations.error.at_command",
            rule = rule + 1,
            command = verb.as_str(),
            why = why
        ),
        (Some(rule), None) => t!("associations.error.at_rule", rule = rule + 1, why = why),
        (None, _) => why.to_owned(),
    }
}

/// The message the ring's stop after a walk turns into.
fn moved(id: Option<Id>) -> Message {
    Message::Associations(AssocMsg::Focused(id))
}

fn focus(id: &'static str) -> Effect {
    Effect::Task(operation::focus(Id::new(id)))
}

impl App {
    /// Reads the rules and switches for the context menu; at the start and when one changes.
    pub(in crate::app) fn associations_task(&self) -> Task<Message> {
        let engine = self.engine.clone();
        Task::perform(async move { engine.file_associations().await }, |read| {
            Message::Associations(AssocMsg::Known(Box::new(read)))
        })
    }

    /// `file.associations`: the window opens on the rules as known, the keyboard in the filter
    /// - or, open already, comes to the front.
    pub(in crate::app) fn open_associations(&mut self) -> Vec<Effect> {
        if self.associations.is_some() {
            return self.dialog_window_front(DialogWindow::Associations);
        }
        self.associations = Some(AssocWindow::new(known::get().rules));
        vec![focus(FILTER_FIELD)]
    }

    /// The window's keys and clicks while it has them; `None` for anything else.
    pub(in crate::app) fn associations_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        let focus = self.associations.as_ref()?.focus;
        let action = match message {
            Message::Key(chord) => keys::key(focus, chord, false),
            Message::KeyCaptured(chord) => keys::key(focus, chord, true),
            Message::Press => return Some(Vec::new()),
            _ => return None,
        };
        Some(match action {
            KeyAction::Nothing => Vec::new(),
            KeyAction::Walk(back) => vec![Effect::WalkFields {
                back,
                then: Some(moved),
            }],
            KeyAction::Follow => vec![Effect::Task(field_walk::focused().map(moved))],
            KeyAction::Run(message) => self.on_associations(message),
        })
    }

    /// A message of the window, or the rules read for the context menu.
    pub(in crate::app) fn on_associations(&mut self, message: AssocMsg) -> Vec<Effect> {
        match message {
            AssocMsg::Known(read) => {
                known::set(*read);
                return Vec::new();
            }
            AssocMsg::Open => return self.open_associations(),
            AssocMsg::Close => {
                self.associations = None;
                return Vec::new();
            }
            AssocMsg::Save => return self.save_associations(),
            _ => {}
        }
        let Some(window) = self.associations.as_mut() else {
            return Vec::new();
        };
        match message {
            AssocMsg::Filter(filter) => window.set_filter(filter),
            AssocMsg::Select(at) => {
                window.select(Some(at));
                window.focus = Focus::List;
            }
            AssocMsg::Step(by) => window.step(by),
            AssocMsg::New => {
                window.add();
                return vec![focus(view::MASK_FIELD)];
            }
            AssocMsg::Delete => window.delete(),
            AssocMsg::Move(up) => window.move_rule(up),
            AssocMsg::Edit(edit) => window.edit(edit),
            AssocMsg::Saved(answer) => {
                window.busy = false;
                window.note = Some(match answer {
                    Ok(()) => {
                        window.dirty = false;
                        (t!("associations.saved").to_owned(), false)
                    }
                    Err(code) => (error_text(code), true),
                });
            }
            AssocMsg::Focused(id) => window.focus = keys::focus_of(id.as_ref()),
            AssocMsg::Open | AssocMsg::Known(_) | AssocMsg::Close | AssocMsg::Save => {}
        }
        Vec::new()
    }

    /// Save: the rules checked, then stored as `files.associations` - the window's way to a
    /// protected setting; the first that does not fit is chosen and the note says why.
    fn save_associations(&mut self) -> Vec<Effect> {
        let Some(window) = self.associations.as_mut() else {
            return Vec::new();
        };
        if window.busy || !window.dirty {
            return Vec::new();
        }
        let checked = match check_rules(&window.rules) {
            Ok(checked) => checked,
            Err(error) => {
                if let Some(rule) = error.rule {
                    window.filter.clear();
                    window.select(Some(rule));
                    let verb = error.verb.as_deref();
                    let command = window
                        .command_names()
                        .iter()
                        .position(|name| Some(name.as_str()) == verb);
                    window.command = command.unwrap_or(0);
                }
                window.note = Some((refusal(&error), true));
                return Vec::new();
            }
        };
        window.rules = checked;
        window.busy = true;
        window.note = None;
        let text = serde_json::Value::from(write_rules(&window.rules));
        let engine = self.engine.clone();
        let stored = async move {
            engine
                .set_protected_setting(FILE_ASSOCIATIONS_KEY, &text)
                .await
                .map(|_| ())
                .map_err(|error| error.code())
        };
        vec![Effect::Task(Task::perform(stored, |answer| {
            Message::Associations(AssocMsg::Saved(answer))
        }))]
    }
}

#[cfg(test)]
#[path = "associations_tests.rs"]
mod tests;
