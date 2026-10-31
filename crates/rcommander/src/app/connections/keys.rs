//! The manager's keyboard. While the dialog is open every key is its own.
//!
//! The manager opens with the keyboard in the tree, on the connection used last (owner,
//! 2026-10-26, `start.rs`; the setting `connections.focus_list` off: in the filter, as before);
//! a character typed in the tree goes into the filter. In the filter, typing filters and a key
//! the field does not use works the tree - the arrows, PageUp and PageDown move its cursor, Enter
//! opens what is under it. Tab and Shift+Tab walk the dialog's focus ring (`field_walk.rs`,
//! RC-080-16): the filter, the tree, the buttons under it, each field of the form, the buttons
//! beside and under it - every control, in reading order; after each step the ring says which
//! stop has the keyboard ([`ConnectionManager::ring_moved`]) and the keys follow it. The tree
//! has them while it is the stop: arrows, Home and End, PageUp and PageDown move the cursor, Left
//! and Right close and open a folder, Enter opens a folder or shows a server in the active panel,
//! Shift+Enter opens a terminal on an SSH server and Ctrl+Enter connects to it (owner, 2026-10-27:
//! no walking to the buttons under the form), F2 renames, F7 makes a folder, F8 or Delete deletes after a question, Insert starts a
//! new connection, Shift+F5 duplicates one, Space marks a connection for the drive bar, Ctrl+X
//! and Ctrl+V move. A text
//! field of the form types, a choice changes with Left, Right or Space; Enter saves. On a button
//! Enter and Space press it. Escape closes the dialog from every stop (RC-080-15); a question in
//! the tree, the master-password prompt and an import page take it first. In the
//! master-password prompt and on the import pages Tab and Shift+Tab walk their own ring. An
//! access key (`theme::mnemonic`) moves the ring as Tab does: the keys follow it the same way.

use iced::widget::{Id, operation};
use rc_engine::{AuthMethod, Chord, ConnectionKind, CredentialId, DefaultView, TreeItem};

use super::{
    ConnMsg, ConnectionManager, Pending,
    form::{Field, Form, theme_choices},
    tree_actions::unfocus,
    view::{FILTER_FIELD, TREE_SCROLL_ID, TREE_STOP_ID},
};
use crate::app::{Effect, Message, field_walk, scroll};

/// Where the manager's keys go: the stop of the focus ring that has the keyboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Tree,
    Filter,
    Field(Field),
    /// A stop whose keys are its own: a button, the reveal switch (RC-080-16).
    Ring,
}

/// Focuses the widget `id`; an id no widget has takes the keys from every text field.
pub(super) fn focus_id(id: &'static str) -> Effect {
    Effect::Task(operation::focus(Id::new(id)))
}

impl ConnectionManager {
    /// A key; `captured` when a text field used it already.
    pub(super) fn key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        let escape = chord.key == "Escape";
        // The master-password prompt and the import pages: Tab walks their fields (the tree and
        // the form walk their own below).
        if (self.prompt.is_some()
            || self.import.is_some()
            || self.tc_import.is_some()
            || self.rdm_import.is_some())
            && let Some(effects) = field_walk::walk_key(chord)
        {
            return effects;
        }
        if self.import.is_some() {
            return self.import_key(chord, captured);
        }
        if self.tc_import.is_some() {
            return self.tc_import_key(chord, captured);
        }
        if self.rdm_import.is_some() {
            return self.rdm_import_key(chord, captured);
        }
        if self.prompt.is_some() {
            if escape {
                self.prompt = None;
                self.close = self.prompt_only;
            }
            return Vec::new();
        }
        match &self.pending {
            // A key the Yes or No button took (the focus ring) is no answer of its own.
            Some(Pending::Delete { .. }) if captured => return Vec::new(),
            Some(Pending::Delete { .. }) => {
                return match (chord.key.as_str(), chord.is_plain()) {
                    ("Enter" | "y", true) => self.confirm_delete(),
                    ("Escape" | "n", true) => {
                        self.pending = None;
                        Vec::new()
                    }
                    _ => Vec::new(),
                };
            }
            Some(Pending::Name { .. }) => {
                if escape {
                    self.pending = None;
                    self.focus = Focus::Tree;
                    return vec![unfocus()];
                }
                return Vec::new();
            }
            None => {}
        }
        // An access key moved the ring to the stop it pressed or gave the keyboard.
        if captured && self.access_key(chord) {
            return vec![follow_ring()];
        }
        // A key a text field typed; Escape never arrives captured while a dialog is open
        // (`app/escape.rs`).
        if captured {
            return Vec::new();
        }
        if let Some(back) = field_walk::tab(chord) {
            return vec![walk(back)];
        }
        match self.focus {
            Focus::Tree => self.tree_key(chord),
            Focus::Filter => self.filter_key(chord),
            Focus::Field(field) => self.field_key(field, chord),
            Focus::Ring => {
                if escape && chord.is_plain() {
                    self.close = true;
                }
                Vec::new()
            }
        }
    }

    /// The ring moved to the stop `id`: the filter, the tree, a field of the form, or another
    /// stop, whose keys are its own.
    pub(super) fn ring_moved(&mut self, id: Option<&Id>) -> Vec<Effect> {
        let is = |name: &'static str| id == Some(&Id::new(name));
        self.focus = if is(FILTER_FIELD) {
            Focus::Filter
        } else if is(TREE_STOP_ID) {
            Focus::Tree
        } else {
            self.form
                .as_ref()
                .and_then(|form| form.fields().into_iter().find(|field| is(field.id())))
                .map_or(Focus::Ring, Focus::Field)
        };
        Vec::new()
    }

    /// Whether `chord`, captured, is an access key that moved the ring: a letter or a digit with
    /// Alt, or alone while a button or a switch has the keyboard (a field types it).
    fn access_key(&self, chord: &Chord) -> bool {
        let mut chars = chord.key.chars();
        let single = chars.next().is_some_and(char::is_alphanumeric) && chars.next().is_none();
        let presses = matches!(
            self.focus,
            Focus::Ring | Focus::Field(Field::ImportKey | Field::Favourite | Field::Forget(_))
        );
        single && !chord.ctrl && (chord.alt || presses)
    }

    fn tree_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let moved = |manager: &mut Self, to: Option<TreeItem>| {
            manager.show(to);
            let rows = manager.tree.rows();
            match manager.tree.cursor_index(&rows) {
                Some(at) => vec![scroll::keep_in_view(TREE_SCROLL_ID, at, rows.len())],
                None => Vec::new(),
            }
        };
        match (chord.key.as_str(), chord.ctrl, chord.shift, chord.alt) {
            ("Escape", false, false, false) => {
                self.close = true;
                Vec::new()
            }
            ("ArrowUp", false, false, false) => {
                let to = self.tree.step(-1);
                moved(self, to)
            }
            ("ArrowDown", false, false, false) => {
                let to = self.tree.step(1);
                moved(self, to)
            }
            ("PageUp", false, false, false) => {
                let to = self.tree.step(-PAGE);
                moved(self, to)
            }
            ("PageDown", false, false, false) => {
                let to = self.tree.step(PAGE);
                moved(self, to)
            }
            ("Home", false, false, false) => {
                let to = self.tree.jump(false);
                moved(self, to)
            }
            ("End", false, false, false) => {
                let to = self.tree.jump(true);
                moved(self, to)
            }
            ("ArrowLeft", false, false, false) => self.left(),
            ("ArrowRight", false, false, false) => {
                if let Some(TreeItem::Folder(id)) = self.tree.current() {
                    self.tree.fold(id, Some(true));
                }
                Vec::new()
            }
            ("Enter", false, false, false) => match self.tree.current() {
                Some(item) => self.activate(item),
                None => Vec::new(),
            },
            ("Enter", false, true, false) => self.terminal_key(),
            ("Enter", true, false, false) => self.connect_key(),
            ("F2", false, false, false) => self.ask_rename(),
            ("F7", false, false, false) => self.ask_name(None),
            ("F8" | "Delete", false, false, false) => self.ask_delete(),
            ("Insert", false, false, false) => self.new_connection(),
            ("F5", false, true, false) => self.duplicate(),
            ("Space", false, false, false) => self.toggle_favourite(),
            ("x", true, false, false) => {
                self.cut();
                Vec::new()
            }
            ("v", true, false, false) => self.paste(),
            _ => self.type_into_filter(chord).unwrap_or_default(),
        }
    }

    /// Left: an open folder closes, anything else goes to its folder.
    fn left(&mut self) -> Vec<Effect> {
        let Some(item) = self.tree.current() else {
            return Vec::new();
        };
        match item {
            TreeItem::Folder(id) if self.tree.is_open(id) => self.tree.fold(id, Some(false)),
            _ => {
                if let Some(parent) = self.tree.parent_of(item) {
                    self.tree.cursor = Some(TreeItem::Folder(parent));
                    self.show(self.tree.cursor);
                }
            }
        }
        Vec::new()
    }

    /// A key the filter field left - one it does not type or edit with: it works the tree while
    /// the filter keeps the keys (the arrows, PageUp and PageDown move the cursor, Enter opens,
    /// Escape closes, F7 or Insert make).
    fn filter_key(&mut self, chord: &Chord) -> Vec<Effect> {
        self.tree_key(chord)
    }

    /// A key while a field of the form is current.
    fn field_key(&mut self, field: Field, chord: &Chord) -> Vec<Effect> {
        let Some(form) = self.form.as_mut() else {
            self.focus = Focus::Tree;
            return Vec::new();
        };
        match (chord.key.as_str(), chord.shift) {
            ("Escape", _) => {
                self.close = true;
                Vec::new()
            }
            ("Enter", false) => self.save(),
            ("ArrowLeft" | "ArrowRight" | "Space", false) => {
                let by = if chord.key == "ArrowLeft" { -1 } else { 1 };
                let mut shared = vec![None];
                shared.extend(self.tree.credentials().iter().map(|entry| Some(entry.id)));
                change(form, field, by, &self.scripts, &shared);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

fn moved(id: Option<Id>) -> Message {
    Message::Connections(ConnMsg::Focused(id))
}

/// Tab (`back`: Shift+Tab): the next stop of the ring, and which stop has the keyboard then.
fn walk(back: bool) -> Effect {
    Effect::WalkFields {
        back,
        then: Some(moved),
    }
}

/// Which stop has the keyboard after an access key moved the ring.
fn follow_ring() -> Effect {
    Effect::Task(field_walk::focused().map(moved))
}

/// Rows a page key moves.
const PAGE: isize = 10;

/// The next (`by` 1) or previous (-1) value of a choice, round; a switch flips. `shared`: no
/// credential, then each shared credential.
fn change(
    form: &mut Form,
    field: Field,
    by: isize,
    scripts: &[(i64, String)],
    shared: &[Option<CredentialId>],
) {
    match field {
        Field::Kind => form.set_kind(cycle(ConnectionKind::ALL, form.kind, by)),
        Field::Auth => form.set_auth(cycle(AuthMethod::ALL, form.auth, by)),
        Field::View => form.set_view(cycle(DefaultView::ALL, form.view, by)),
        Field::Script => {
            let mut choices = vec![None];
            choices.extend(scripts.iter().map(|(id, _)| Some(*id)));
            form.set_script(cycle(&choices, form.script, by));
        }
        Field::Credential => form.set_shared(cycle(shared, form.shared, by)),
        Field::ThemeLight | Field::ThemeDark => {
            let dark = field == Field::ThemeDark;
            let mut choices = vec![None];
            choices.extend(theme_choices(dark).into_iter().map(|(id, _)| Some(id)));
            let at = choices
                .iter()
                .position(|choice| choice.as_deref() == form.theme(dark))
                .unwrap_or(0);
            let places: Vec<usize> = (0..choices.len()).collect();
            let next = cycle(&places, at, by);
            form.set_theme(dark, choices.swap_remove(next));
        }
        Field::ImportKey | Field::Favourite | Field::Forget(_) => form.toggle(field),
        _ => {}
    }
}

/// The value `by` steps from `current` in `all`, wrapping round.
pub(super) fn cycle<T: Copy + PartialEq>(all: &[T], current: T, by: isize) -> T {
    let Some(at) = all.iter().position(|value| *value == current) else {
        return all.first().copied().unwrap_or(current);
    };
    let len = all.len() as isize;
    all[(at as isize + by).rem_euclid(len) as usize]
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
