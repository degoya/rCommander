//! The keymap: which key chord runs which command, kept as data.
//!
//! The standard map is `keymap/default.json`, compiled in: a list of bindings, each a chord
//! (`"keys": "Ctrl+Shift+F"`), a registry command with its parameters (`"command"` and
//! `"params"`, the serde form of [`Command`]) and when it applies (`"when"`: `files`, the default,
//! while the panel shows its file list; `always`, also while a terminal has the keyboard). A row
//! with `"reserved"` instead of a command keeps a Total Commander key free for the job that brings
//! its feature (`{"keys": "F5", "reserved": "copy (RC-010-07)"}`): that job only swaps the
//! reservation for its command. A row with `"window"` names a key the window handles itself,
//! because what it does is the window's own state (the command line, the command palette):
//! `{"keys": "Ctrl+P", "window": "cmdline_insert_folder"}`. Every row has a name, the i18n key
//! `keys.<name>` the command palette and the keyboard reference show: a window row's value, a
//! binding's `"name"` or else its command id (a binding with parameters names itself, so Ctrl+F3
//! reads "Sort by name", not "Sort"). The window turns a key press into a [`Chord`] and asks
//! [`Keymap::lookup`] and [`Keymap::window_key`]. The keyboard reference
//! (`scripts/keyboard-reference.sh`) reads the same data. The keymap in force is the standard
//! with the person's deviations from it (RC-100-21): `overrides.rs` keeps them as the setting
//! `keys.overrides`, `edit.rs` applies them and makes the Shortcuts tab's edits. A key whose
//! character the layout decides can be bound by where it sits, `Ctrl+[Semicolon]` (RC-100-48,
//! `physical.rs`).

mod edit;
mod overrides;
mod physical;

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use self::edit::{KeymapRow, Lock, Refusal};
pub use self::overrides::{MAX_OVERRIDE_TEXT, MAX_OVERRIDES, Override, Overrides};
pub use self::physical::{PHYSICAL_KEYS, physical_code};
use crate::{Command, PanelView};

/// The standard bindings: Total Commander 11's, plus the key right of L (Ctrl+O-umlaut on a
/// German layout, Ctrl+; on a US one) for the terminal.
const DEFAULT: &str = include_str!("../keymap/default.json");

/// The named keys a chord can use: the W3C `KeyboardEvent.key` names, which iced reports.
const NAMED_KEYS: &[&str] = &[
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "Backspace",
    // The context-menu key beside the right Ctrl (Shift+F10 does the same).
    "ContextMenu",
    "Delete",
    "End",
    "Enter",
    "Escape",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "Home",
    "Insert",
    // The keypad's grey keys, told from `+`, `-` and `*` elsewhere by where they sit.
    "NumAdd",
    "NumDivide",
    "NumMultiply",
    "NumSubtract",
    "PageDown",
    "PageUp",
    // The `+` of the main block, a key of its own on a German layout (RC-020-12): `+` joins the
    // parts of a chord and cannot name a key.
    "Plus",
    "Space",
    "Tab",
];

/// A key with its modifiers, written `Tab`, `Ctrl+PageUp`, ``Ctrl+` `` or `Ctrl+[Semicolon]`.
/// Equal by key and modifiers alone (`physical.rs`).
#[derive(Debug, Clone)]
pub struct Chord {
    /// A named key (`Enter`, `F5`, `ArrowUp`), one character in lower case (`a`, `` ` ``) or a
    /// key named by where it sits (`[Semicolon]`, [`PHYSICAL_KEYS`]).
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// The code of the key pressed, when a chord can name that key by place: the window sets it,
    /// so a binding on `[Semicolon]` finds the key whatever character the layout gives it. Never
    /// read from text nor printed.
    pub physical: Option<&'static str>,
}

impl Chord {
    /// `key` without modifiers, when it is a key a chord can name ([`Chord::key_name`]).
    #[must_use]
    pub fn plain(key: &str) -> Option<Self> {
        Some(Self {
            key: Self::key_name(key)?,
            ctrl: false,
            shift: false,
            alt: false,
            physical: None,
        })
    }

    /// No modifier held: neither Ctrl, Shift nor Alt.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        !(self.ctrl || self.shift || self.alt)
    }

    /// How a chord spells `key`: a named key and a key named by place as they are, one
    /// character in lower case; `None` for anything else.
    #[must_use]
    pub fn key_name(key: &str) -> Option<String> {
        let mut chars = key.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if !c.is_whitespace() && c != '+' => Some(c.to_lowercase().collect()),
            _ => (NAMED_KEYS.contains(&key) || physical::bracketed(key).is_some())
                .then(|| key.to_owned()),
        }
    }
}

impl FromStr for Chord {
    type Err = KeymapError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wrong = || KeymapError::Chord(text.to_owned());
        let (modifiers, key) = text.rsplit_once('+').unwrap_or(("", text));
        let mut chord = Self::plain(key).ok_or_else(wrong)?;
        for modifier in modifiers.split('+').filter(|part| !part.is_empty()) {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                _ => return Err(wrong()),
            }
        }
        Ok(chord)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [
            (self.ctrl, "Ctrl+"),
            (self.shift, "Shift+"),
            (self.alt, "Alt+"),
        ] {
            if held {
                formatter.write_str(name)?;
            }
        }
        if self.key.chars().count() == 1 {
            formatter.write_str(&self.key.to_uppercase())
        } else {
            formatter.write_str(&self.key)
        }
    }
}

/// When a binding applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyContext {
    /// While the active panel shows its file list; a terminal gets the key itself.
    #[default]
    Files,
    /// Whatever the active panel shows.
    Always,
}

impl KeyContext {
    /// Whether a binding of this context applies to a panel showing `view`.
    #[must_use]
    pub fn applies(self, view: PanelView) -> bool {
        self == Self::Always || view == PanelView::Files
    }
}

/// One row of the keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub chord: Chord,
    pub when: KeyContext,
    pub command: Command,
    /// The name's i18n key is `keys.<name>`: the row's `"name"`, or else the command id.
    pub name: String,
}

/// A chord kept free for a feature that arrives later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reserved {
    pub chord: Chord,
    /// What it is kept for, and the job that brings it.
    pub purpose: String,
}

/// A chord the window handles itself: what it does is the window's own state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowKey {
    pub chord: Chord,
    pub when: KeyContext,
    /// What it does; the name's i18n key is `keys.<name>`.
    pub name: String,
}

/// A row as `keymap/default.json` writes it: a command, a reservation or a window key.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBinding {
    keys: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    when: KeyContext,
    #[serde(default)]
    reserved: Option<String>,
    #[serde(default)]
    window: Option<String>,
}

/// Whether `name` can follow `keys.` in an i18n key: lower-case letters, digits, `_` and `.`.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// Why a keymap does not read.
#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    #[error("the keymap is not a list of bindings: {0}")]
    Json(#[source] serde_json::Error),
    #[error("{0:?} is not a key chord")]
    Chord(String),
    #[error("{keys}: not a command of the registry: {source}")]
    Command {
        keys: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{keys} is bound twice")]
    Duplicate { keys: String },
    #[error(
        "{keys}: a row names one of a command, a reservation or a window key, and a valid name"
    )]
    Row { keys: String },
    #[error("{0:?} is no key override: write <keys>=<name>, or <keys>= for none")]
    Override(String),
    #[error("{keys} is the window's own key and cannot be bound")]
    Locked { keys: String },
    #[error(
        "the key overrides are more than {MAX_OVERRIDES} words or {MAX_OVERRIDE_TEXT} characters"
    )]
    TooLong,
}

/// The bindings, the reserved chords and the window's keys, in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<Binding>,
    reserved: Vec<Reserved>,
    window: Vec<WindowKey>,
}

impl Keymap {
    /// A keymap from its JSON form.
    ///
    /// # Errors
    ///
    /// The text is no list of bindings, a chord does not read, a command is not in the registry
    /// or its parameters do not fit, a row names more than one of a command, a reservation and
    /// a window key or none of them, a name is no i18n key part, or a chord is taken twice.
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        let raw: Vec<RawBinding> = serde_json::from_str(text).map_err(KeymapError::Json)?;
        let mut keymap = Self {
            bindings: Vec::with_capacity(raw.len()),
            reserved: Vec::new(),
            window: Vec::new(),
        };
        for row in raw {
            let chord: Chord = row.keys.parse()?;
            if keymap.takes(&chord) {
                return Err(KeymapError::Duplicate { keys: row.keys });
            }
            let bare = row.params.is_none() && row.name.is_none();
            match (row.command, row.reserved, row.window) {
                (Some(id), None, None) => {
                    let name = row.name.unwrap_or_else(|| id.clone());
                    if !is_name(&name) {
                        return Err(KeymapError::Row { keys: row.keys });
                    }
                    let params = row.params.unwrap_or_else(|| json!({}));
                    let command =
                        serde_json::from_value(json!({ "command": id, "params": params }))
                            .map_err(|source| KeymapError::Command {
                                keys: row.keys.clone(),
                                source,
                            })?;
                    keymap.bindings.push(Binding {
                        chord,
                        when: row.when,
                        command,
                        name,
                    });
                }
                (None, Some(purpose), None) if bare => {
                    keymap.reserved.push(Reserved { chord, purpose });
                }
                (None, None, Some(name)) if bare && is_name(&name) => {
                    keymap.window.push(WindowKey {
                        chord,
                        when: row.when,
                        name,
                    });
                }
                _ => return Err(KeymapError::Row { keys: row.keys }),
            }
        }
        Ok(keymap)
    }

    /// Whether `chord` is bound, reserved or the window's.
    fn takes(&self, chord: &Chord) -> bool {
        self.bindings.iter().any(|binding| binding.chord == *chord)
            || self
                .reserved
                .iter()
                .any(|reserved| reserved.chord == *chord)
            || self.window.iter().any(|key| key.chord == *chord)
    }

    /// The standard keymap.
    #[must_use]
    pub fn standard() -> Self {
        Self::shared_standard().clone()
    }

    /// Every binding, in the order written.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The chords kept free for later features, in the order written.
    #[must_use]
    pub fn reserved(&self) -> &[Reserved] {
        &self.reserved
    }

    /// The keys the window handles itself, in the order written.
    #[must_use]
    pub fn window(&self) -> &[WindowKey] {
        &self.window
    }

    /// The command the key press `chord` runs while the active panel shows `view`; a reserved
    /// chord and a window key run none. A binding on the key's character goes before one on its
    /// place ([`Chord::matches`]): a key the person bound by its character keeps it.
    #[must_use]
    pub fn lookup(&self, chord: &Chord, view: PanelView) -> Option<&Command> {
        let rows = || {
            self.bindings
                .iter()
                .filter(move |binding| binding.when.applies(view))
        };
        rows()
            .find(|binding| binding.chord == *chord)
            .or_else(|| rows().find(|binding| binding.chord.matches(chord)))
            .map(|binding| &binding.command)
    }

    /// The name of the window key the key press `chord` is while the active panel shows `view`;
    /// as [`Keymap::lookup`], its character before its place.
    #[must_use]
    pub fn window_key(&self, chord: &Chord, view: PanelView) -> Option<&str> {
        let rows = || self.window.iter().filter(move |key| key.when.applies(view));
        rows()
            .find(|key| key.chord == *chord)
            .or_else(|| rows().find(|key| key.chord.matches(chord)))
            .map(|key| key.name.as_str())
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::standard()
    }
}

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "keymap_feature_tests.rs"]
mod feature_tests;

#[cfg(test)]
#[path = "keymap_window_tests.rs"]
mod window_tests;

#[cfg(test)]
#[path = "keymap_tc_tests.rs"]
mod tc_tests;

#[cfg(test)]
#[path = "keymap_physical_tests.rs"]
mod physical_tests;
