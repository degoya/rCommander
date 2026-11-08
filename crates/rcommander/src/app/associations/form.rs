//! What the window edits (RC-100-91): every rule at once, in the order they are tried, the rule at
//! the cursor and which of its commands the form shows - `open` (Enter's) first, then the named
//! ones. Nothing here is checked: Save hands the rules to `rc_engine::associations::check_rules`,
//! which says why they do not fit.

use rc_engine::{AssociationVerb, CommandRef, FileAssociation};

use super::keys::Focus;
use crate::app::user_commands::{CmdMsg, edit};

/// Rows a PageUp or PageDown moves.
pub const PAGE: isize = 10;

/// A change of the rule at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleMsg {
    Mask(String),
    Description(String),
    Icon(String),
    /// The form shows this command: 0 is `open`, then the named ones.
    Command(usize),
    /// A new named command, shown.
    AddCommand,
    /// The command shown goes: `open` is emptied, a named one removed.
    RemoveCommand,
    /// The shown named command's name.
    Name(String),
    Cmd(CmdMsg),
}

/// The open window.
#[derive(Debug, Default)]
pub struct AssocWindow {
    pub rules: Vec<FileAssociation>,
    pub filter: String,
    /// The rule at the cursor, by its place in `rules`.
    pub cursor: Option<usize>,
    /// The command of that rule the form shows: 0 is `open`.
    pub command: usize,
    pub focus: Focus,
    /// The last outcome: a text, and whether it is an error.
    pub note: Option<(String, bool)>,
    /// Changes not saved yet.
    pub dirty: bool,
    /// A save runs.
    pub busy: bool,
}

/// Whether `rule` shows under `filter`: its mask or its description holds it, case ignored.
fn shows(rule: &FileAssociation, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || rule.mask.to_lowercase().contains(&filter)
        || rule.description.to_lowercase().contains(&filter)
}

impl AssocWindow {
    /// The window over `rules`, the cursor on the first.
    pub fn new(rules: Vec<FileAssociation>) -> Self {
        let cursor = (!rules.is_empty()).then_some(0);
        Self {
            rules,
            cursor,
            ..Self::default()
        }
    }

    /// The places of the rules the filter shows, in order.
    pub fn shown(&self) -> Vec<usize> {
        (0..self.rules.len())
            .filter(|&at| shows(&self.rules[at], &self.filter))
            .collect()
    }

    /// The rule at the cursor.
    pub fn rule(&self) -> Option<&FileAssociation> {
        self.cursor.and_then(|at| self.rules.get(at))
    }

    /// The names of the rule's commands as the form's choice lists them: `open`, then the named
    /// ones.
    pub fn command_names(&self) -> Vec<String> {
        let mut names = vec![rc_engine::associations::OPEN_VERB.to_owned()];
        if let Some(rule) = self.rule() {
            names.extend(rule.verbs.iter().map(|verb| verb.name.clone()));
        }
        names
    }

    /// The command the form shows; an `open` not set yet is an empty program.
    pub fn action(&self) -> CommandRef {
        let Some(rule) = self.rule() else {
            return CommandRef::default();
        };
        match self.command.checked_sub(1) {
            None => rule.open.clone().unwrap_or_default(),
            Some(at) => rule
                .verbs
                .get(at)
                .map(|verb| verb.action.clone())
                .unwrap_or_default(),
        }
    }

    /// The new filter; the cursor stays on a rule it shows, else on the first one shown.
    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        let shown = self.shown();
        if self.cursor.is_none_or(|at| !shown.contains(&at)) {
            self.select(shown.first().copied());
        }
    }

    /// The cursor to the rule at `at`, its `open` shown.
    pub fn select(&mut self, at: Option<usize>) {
        self.cursor = at.filter(|at| *at < self.rules.len());
        self.command = 0;
    }

    /// The cursor `by` rows among those shown: up for less than 0, clamped to the ends.
    pub fn step(&mut self, by: isize) {
        let shown = self.shown();
        let Some(last) = shown.len().checked_sub(1) else {
            return;
        };
        let now = self
            .cursor
            .and_then(|at| shown.iter().position(|place| *place == at));
        let next = match now {
            None => 0,
            Some(row) => row.saturating_add_signed(by).min(last),
        };
        if Some(shown[next]) != self.cursor {
            self.select(Some(shown[next]));
        }
    }

    /// A new rule after the cursor's (at the end without one), the filter cleared so it shows.
    pub fn add(&mut self) {
        let at = self.cursor.map_or(self.rules.len(), |at| at + 1);
        self.rules.insert(at, FileAssociation::default());
        self.filter.clear();
        self.select(Some(at));
        self.dirty = true;
    }

    /// The rule at the cursor goes; the cursor to the one after it, else before it.
    pub fn delete(&mut self) {
        let Some(at) = self.cursor.filter(|at| *at < self.rules.len()) else {
            return;
        };
        self.rules.remove(at);
        let shown = self.shown();
        let next = shown
            .iter()
            .copied()
            .find(|place| *place >= at)
            .or_else(|| shown.last().copied());
        self.select(next);
        self.dirty = true;
    }

    /// The rule at the cursor one place up (`up`) or down among all rules: the order they are
    /// tried in. The cursor goes along.
    pub fn move_rule(&mut self, up: bool) {
        let Some(at) = self.cursor else {
            return;
        };
        let other = if up { at.checked_sub(1) } else { Some(at + 1) };
        let Some(other) = other.filter(|other| *other < self.rules.len()) else {
            return;
        };
        self.rules.swap(at, other);
        self.cursor = Some(other);
        self.dirty = true;
    }

    /// `message` applied to the rule at the cursor.
    pub fn edit(&mut self, message: RuleMsg) {
        let command = self.command;
        let Some(rule) = self.cursor.and_then(|at| self.rules.get_mut(at)) else {
            return;
        };
        match message {
            RuleMsg::Mask(text) => rule.mask = text,
            RuleMsg::Description(text) => rule.description = text,
            RuleMsg::Icon(text) => rule.icon = text,
            RuleMsg::Command(at) => {
                self.command = at.min(rule.verbs.len());
                return;
            }
            RuleMsg::AddCommand => {
                rule.verbs.push(AssociationVerb::default());
                self.command = rule.verbs.len();
            }
            RuleMsg::RemoveCommand => match command.checked_sub(1) {
                None => rule.open = None,
                Some(at) if at < rule.verbs.len() => {
                    rule.verbs.remove(at);
                    self.command = at.min(rule.verbs.len());
                }
                Some(_) => return,
            },
            RuleMsg::Name(name) => match command.checked_sub(1) {
                Some(at) if at < rule.verbs.len() => rule.verbs[at].name = name,
                _ => return,
            },
            RuleMsg::Cmd(message) => match command.checked_sub(1) {
                None => edit(rule.open.get_or_insert_with(CommandRef::default), message),
                Some(at) if at < rule.verbs.len() => edit(&mut rule.verbs[at].action, message),
                Some(_) => return,
            },
        }
        self.dirty = true;
    }
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
