//! The command palette (RC-080-05): Ctrl+Shift+P, a window key of the keymap, opens a field over
//! the window that finds every action by its name or its key - the keymap's bindings and window
//! keys, the commands without a key the palette runs itself (`reach.json`), and the settings, the
//! database dialog with its export and import (RC-080-19) and the settings' About page
//! (RC-010-19), which have no key, and the divider between the panels back to 50 % (RC-010-20).
//! Typing narrows the list, ArrowUp and ArrowDown (PageUp, PageDown) choose, Enter runs, Escape
//! closes; a click runs a row too.
//!
//! A row with a key runs exactly as pressing the key would: the palette closes and the chord
//! goes to [`App::on_key`]. The names are the keymap's i18n keys (`keys.<name>`), so the palette,
//! the function keys and the keyboard reference say the same. The palette itself is a window
//! key, not a registry command: there is nothing in it for MCP, which calls the commands directly.
//! The menu bar (`menu_bar.rs`, RC-080-18) runs the palette's rows by their names.

pub mod menu_bar;
pub mod reach;
mod view;

use iced::widget::{Id, operation};
use rc_engine::{Chord, Command, Keymap};

use self::reach::{Reach, ReachRow};
pub(super) use self::view::overlay;
use super::{
    App, Effect, Message,
    cmdline::CmdLineMsg,
    database_dialog::{DatabaseMsg, DialogMode},
    drive_bar::{DriveMessage, actions::DriveAction},
    resize::{ResizeMsg, Target},
    settings::{Page, SettingsMessage},
};
use crate::i18n;

/// The id of the palette's text field, to focus it.
pub const FIELD_ID: &str = "rcommander.palette";

/// The window key that opens the palette.
pub const OPEN_KEY: &str = "palette";

/// How many rows a page is, for PageUp and PageDown and for drawing.
pub const PAGE: usize = 12;

/// A dialog of the window without a key of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Settings,
    Database,
    /// The database dialog on the export or the import of settings and connections (RC-080-19).
    Exchange(DialogMode),
    /// The settings' About page (RC-010-19).
    About,
    /// The network drive dialogs (RC-010-16), where the platform maps network drives.
    Network(DriveAction),
}

/// What running a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// What pressing the chord does.
    Press(Chord),
    /// A command without a key, for the active panel; boxed, a command is large.
    Run(Box<Command>),
    /// A binding whose keys were all taken away, run as its key ran it (RC-100-21).
    Bound(Box<Command>),
    Open(Surface),
    /// The divider between the panels to the middle, as a double-click on it (RC-010-20).
    CenterDivider,
}

/// One row of the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The i18n key's part after `keys.`.
    pub name: String,
    pub label: String,
    /// Its chords, as the keymap prints them (`Backspace, Ctrl+PageUp`); empty without a key.
    pub keys: String,
    pub action: Action,
}

/// The open palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub query: String,
    entries: Vec<Entry>,
    /// The entries the query matches, by index, in the order shown.
    pub shown: Vec<usize>,
    /// The chosen row of `shown`.
    pub selected: usize,
}

/// The palette's messages from its widgets.
#[derive(Debug, Clone)]
pub enum PaletteMsg {
    Query(String),
    /// A click on row `index` of the shown ones.
    Pick(usize),
    /// The menu bar: its setting read, an entry chosen (RC-080-18).
    MenuBar(menu_bar::MenuBarMsg),
}

/// Every row of the palette, by label: one per name of the keymap with all its chords - a
/// standard name whose keys were taken away without one -, the window keys but the palette's
/// own, the palette commands of `rows` and the dialogs without a key - the network drive dialogs
/// only where `network` drives are mapped.
pub fn entries(keymap: &Keymap, rows: &[ReachRow], network: bool) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut add = |name: &str, chord: &Chord| {
        let known = entries.iter().position(|entry| entry.name == name);
        match known {
            Some(at) => entries[at].keys = format!("{}, {chord}", entries[at].keys),
            None => entries.push(entry(name, chord.to_string(), Action::Press(chord.clone()))),
        }
    };
    for binding in keymap.bindings() {
        add(&binding.name, &binding.chord);
    }
    for key in keymap.window().iter().filter(|key| key.name != OPEN_KEY) {
        add(&key.name, &key.chord);
    }
    for binding in keymap.without_key() {
        let command = Box::new(binding.command.clone());
        entries.push(entry(&binding.name, String::new(), Action::Bound(command)));
    }
    for row in rows.iter().filter(|row| row.reach == Reach::Palette) {
        if let Some(command) = reach::palette_command(&row.command) {
            entries.push(entry(
                &row.command,
                String::new(),
                Action::Run(Box::new(command)),
            ));
        }
    }
    entries.push(entry(
        "settings",
        String::new(),
        Action::Open(Surface::Settings),
    ));
    entries.push(entry(
        "database",
        String::new(),
        Action::Open(Surface::Database),
    ));
    for (name, mode) in [
        ("config_export", DialogMode::Export),
        ("config_import", DialogMode::Import),
    ] {
        entries.push(entry(
            name,
            String::new(),
            Action::Open(Surface::Exchange(mode)),
        ));
    }
    entries.push(entry("about", String::new(), Action::Open(Surface::About)));
    entries.push(entry(
        "center_divider",
        String::new(),
        Action::CenterDivider,
    ));
    if network {
        for (name, action) in [
            ("map_network_drive", DriveAction::MapNetwork),
            ("unmap_network_drive", DriveAction::UnmapNetwork),
        ] {
            entries.push(entry(
                name,
                String::new(),
                Action::Open(Surface::Network(action)),
            ));
        }
    }
    entries.sort_by_key(|entry| entry.label.to_lowercase());
    entries
}

fn entry(name: &str, keys: String, action: Action) -> Entry {
    Entry {
        name: name.to_owned(),
        label: i18n::code_text("keys", name),
        keys,
        action,
    }
}

impl Palette {
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut palette = Self {
            query: String::new(),
            entries,
            shown: Vec::new(),
            selected: 0,
        };
        palette.filter();
        palette
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// The entries whose label or keys hold every word of the query, ignoring case.
    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        self.shown = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let text = format!("{} {}", entry.label, entry.keys).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.filter();
    }

    /// Moves the chosen row by `by`, staying in the list.
    fn step(&mut self, by: isize) {
        let last = self.shown.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(by).min(last);
    }

    /// The action of shown row `index`.
    fn action(&self, index: usize) -> Option<Action> {
        let entry = self.shown.get(index)?;
        self.entries.get(*entry).map(|entry| entry.action.clone())
    }
}

/// What a key does to the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteKey {
    Stay,
    Close,
    Run(Action),
}

/// The palette's own keys; every other key stays with its field.
pub fn key(palette: &mut Palette, chord: &Chord) -> PaletteKey {
    let plain = chord.is_plain();
    let page = PAGE as isize;
    match chord.key.as_str() {
        "Escape" => return PaletteKey::Close,
        "Enter" if plain => {
            return palette
                .action(palette.selected)
                .map_or(PaletteKey::Stay, PaletteKey::Run);
        }
        "ArrowDown" if plain => palette.step(1),
        "ArrowUp" if plain => palette.step(-1),
        "PageDown" if plain => palette.step(page),
        "PageUp" if plain => palette.step(-page),
        _ => {}
    }
    PaletteKey::Stay
}

impl App {
    /// Ctrl+Shift+P: the palette opens with every row, its field focused.
    pub(super) fn open_palette(&mut self) -> Vec<Effect> {
        let entries = entries(&self.keymap, &reach::standard(), self.drive_bar.network);
        self.palette = Some(Palette::new(entries));
        vec![Effect::Task(operation::focus(Id::new(FIELD_ID)))]
    }

    /// While the palette is open it has the keys - typed ones too - and the clicks.
    pub(super) fn palette_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        let palette = self.palette.as_mut()?;
        match message {
            Message::Key(chord) | Message::KeyCaptured(chord) => match key(palette, chord) {
                PaletteKey::Stay => Some(Vec::new()),
                PaletteKey::Close => {
                    self.palette = None;
                    Some(Vec::new())
                }
                PaletteKey::Run(action) => Some(self.run_palette(action)),
            },
            // A key that typed while the field lost the keyboard still types into it.
            Message::CmdLine(CmdLineMsg::Typed(_, text)) => {
                let query = format!("{}{text}", palette.query);
                palette.set_query(query);
                Some(vec![Effect::Task(operation::focus(Id::new(FIELD_ID)))])
            }
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    pub(super) fn on_palette(&mut self, message: PaletteMsg) -> Vec<Effect> {
        if let PaletteMsg::MenuBar(message) = message {
            return self.on_menu_bar(message);
        }
        let Some(palette) = self.palette.as_mut() else {
            return Vec::new();
        };
        match message {
            PaletteMsg::Query(query) => palette.set_query(query),
            PaletteMsg::Pick(index) => {
                if let Some(action) = palette.action(index) {
                    return self.run_palette(action);
                }
            }
            PaletteMsg::MenuBar(_) => {}
        }
        Vec::new()
    }

    /// Closes the palette and runs `action`: a chord as if pressed, a command for the active
    /// panel, or a dialog opened.
    fn run_palette(&mut self, action: Action) -> Vec<Effect> {
        self.palette = None;
        match action {
            Action::Press(chord) => self.on_key(chord, false),
            Action::Run(command) => vec![Effect::Dispatch(self.active, vec![*command])],
            Action::Bound(command) => self.run_bound(*command),
            Action::Open(Surface::Settings) => self.apply(Message::Settings(SettingsMessage::Open)),
            Action::Open(Surface::Database) => self.apply(Message::Database(DatabaseMsg::Open)),
            Action::Open(Surface::Exchange(mode)) => {
                self.apply(Message::Database(DatabaseMsg::OpenMode(mode)))
            }
            Action::Open(Surface::About) => {
                self.apply(Message::Settings(SettingsMessage::OpenPage(Page::About)))
            }
            Action::Open(Surface::Network(action)) => {
                self.apply(Message::Drive(DriveMessage::Run(self.active, action, None)))
            }
            Action::CenterDivider => self.apply(Message::Resize(ResizeMsg::Reset(Target::Divider))),
        }
    }
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
