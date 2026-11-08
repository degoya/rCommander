//! The button bars' window (RC-100-87, `DialogWindow::ButtonBar`): Configuration > "Button
//! bar...", the palette and a button's menu open it. Total Commander's "Change button bar" as one
//! window: the bar to edit (a choice of the bars, New, Delete, "Import .bar..."), its name, its
//! items in a list - Ctrl+Up and Ctrl+Down move the one at the cursor -, the form of the item at
//! the cursor (a button's command as the own commands' window edits one - program with Browse,
//! internal rows or an own command, parameters, start folder, window, run as administrator - or
//! the sub-bar it opens, its symbol and tooltip), "Use as button bar" and "Use as vertical bar",
//! Save and Close; "Choose symbol..." beside a button's symbol opens the symbol picker over the
//! form (`window/symbol.rs`, RC-110-10). The import shows what a `.bar` file and its sub-bars
//! would make, item by item with what is skipped and why, before anything is stored.
//!
//! The window keeps no bar of its own: it edits a copy of one of the engine's, and every change is
//! a command (`button_bar.save`, `.delete`, `.import_preview`, `.import`, `settings.set`).

mod symbol;
mod view;

use iced::{
    Task,
    widget::{Id, operation},
};
use rc_engine::{
    BarButton, BarItem, BarPlace, BarSpec, ButtonAction, ButtonBar, ButtonBarId, ButtonBarsOutcome,
    Command, CommandRef, Outcome,
    button_bars::ImportPreview,
    settings::{HORIZONTAL_BAR_KEY, VERTICAL_BAR_KEY},
};

pub use self::view::{NAME_FIELD, body, layer};
use super::{App, BbMsg, Effect, wrap};
use crate::app::{
    Message,
    dialog_windows::DialogWindow,
    symbol_picker::{PickerMsg, SymbolPicker},
    user_commands::{CmdMsg, edit_command},
};

/// What a form's button is: a command, or a sub-bar it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Does {
    Command,
    SubBar,
}

/// What an answer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Saved,
    Deleted,
    Preview,
    Imported,
    Used,
}

/// The import of a `.bar` file: the path typed, and what it would make once read.
#[derive(Debug, Clone, Default)]
pub struct Import {
    pub path: String,
    pub preview: Option<ImportPreview>,
}

/// The open window.
#[derive(Debug, Clone, Default)]
pub struct BarWindow {
    /// The bar edited; `None` for a new one.
    pub bar: Option<ButtonBarId>,
    pub name: String,
    pub items: Vec<BarItem>,
    /// The item at the cursor.
    pub cursor: Option<usize>,
    pub dirty: bool,
    /// Delete the bar? asked.
    pub asking: bool,
    pub import: Option<Import>,
    /// The last outcome: a text, and whether it is an error.
    pub note: Option<(String, bool)>,
    pub busy: bool,
    /// "Choose symbol..." open over the button's form (RC-110-10).
    pub picker: Option<SymbolPicker>,
}

/// What the window's controls and the engine's answers say.
#[derive(Debug, Clone)]
pub enum WinMsg {
    Answer(Then, Box<Result<Outcome, &'static str>>),
    Close,
    /// Another bar chosen.
    Bar(ButtonBarId),
    NewBar,
    Name(String),
    DeleteBar,
    Confirm(bool),
    /// The bar edited shows at this place from now on.
    UseAs(BarPlace),
    Select(usize),
    /// The cursor moved by rows (Up, Down).
    Step(isize),
    /// The item at the cursor moved by rows (Ctrl+Up, Ctrl+Down).
    Shift(isize),
    AddButton,
    AddSeparator,
    Remove,
    Does(Does),
    Cmd(CmdMsg),
    SubBar(ButtonBarId),
    Tooltip(String),
    IconPath(String),
    IconIndex(String),
    ImportOpen,
    ImportPath(String),
    ImportRead,
    ImportRun,
    ImportCancel,
    Save,
    /// "Choose symbol..." beside the button's symbol (RC-110-10).
    ChooseSymbol,
    Picker(PickerMsg),
}

fn win(message: WinMsg) -> Message {
    wrap(BbMsg::Window(message))
}

/// `command`, its answer as [`WinMsg::Answer`] for `then`.
fn call(app: &App, then: Then, command: Command) -> Effect {
    let engine = app.engine.clone();
    Effect::Task(Task::perform(
        async move { engine.dispatch(command).await.map_err(|error| error.code()) },
        move |answer| win(WinMsg::Answer(then, Box::new(answer))),
    ))
}

impl BarWindow {
    /// The window on `bar`, its items as stored.
    fn of(bar: &ButtonBar) -> Self {
        Self {
            bar: Some(bar.id),
            name: bar.name.clone(),
            items: bar.spec.items.clone(),
            cursor: (!bar.spec.items.is_empty()).then_some(0),
            ..Self::default()
        }
    }

    /// New bars from the engine: unchanged edits read the bar again; a bar gone becomes new.
    pub fn refresh(&mut self, bars: &[ButtonBar]) {
        if self.dirty {
            return;
        }
        match self.bar.and_then(|id| bars.iter().find(|bar| bar.id == id)) {
            Some(bar) => {
                let cursor = self.cursor;
                *self = Self {
                    note: self.note.take(),
                    import: self.import.take(),
                    ..Self::of(bar)
                };
                self.cursor = cursor.filter(|at| *at < self.items.len()).or(self.cursor);
            }
            None if self.bar.is_some() => *self = Self::default(),
            None => {}
        }
    }

    /// The button at the cursor.
    fn button_mut(&mut self) -> Option<&mut BarButton> {
        match self.items.get_mut(self.cursor?)? {
            BarItem::Button(button) => Some(button),
            BarItem::Separator => None,
        }
    }

    /// The spec saved.
    fn spec(&self) -> BarSpec {
        BarSpec {
            items: self.items.clone(),
            ..BarSpec::default()
        }
    }

    /// A new item after the cursor, the cursor on it.
    fn insert(&mut self, item: BarItem) {
        let at = self.cursor.map_or(self.items.len(), |at| at + 1);
        self.items.insert(at.min(self.items.len()), item);
        self.cursor = Some(at.min(self.items.len() - 1));
        self.dirty = true;
    }
}

/// A new button: a program to choose.
fn blank_button() -> BarButton {
    BarButton {
        action: ButtonAction::Command(CommandRef::default()),
        icon: None,
        tooltip: String::new(),
    }
}

impl App {
    /// Opens the window on `at` - a bar and the item to put the cursor on - or on the bar the
    /// button bar shows; brings it to the front when open.
    pub(in crate::app) fn open_button_bar_window(
        &mut self,
        at: Option<(ButtonBarId, Option<usize>)>,
    ) -> Vec<Effect> {
        let bar = match at {
            Some((id, _)) => self.button_bar.bar(id),
            None => self.button_bar.shown(BarPlace::Horizontal),
        }
        .or_else(|| self.button_bar.bars.first());
        let mut window = bar.map(BarWindow::of).unwrap_or_default();
        if let Some((_, Some(item))) = at
            && item < window.items.len()
        {
            window.cursor = Some(item);
        }
        let front = self.button_bar.window.is_some();
        self.button_bar.window = Some(window);
        let mut effects = if front {
            self.dialog_window_front(DialogWindow::ButtonBar)
        } else {
            Vec::new()
        };
        effects.push(Effect::Task(operation::focus(Id::new(NAME_FIELD))));
        effects
    }

    /// "Insert button": the window on bar `bar` with a new button after `after`.
    pub(in crate::app) fn insert_in_bar_window(
        &mut self,
        bar: ButtonBarId,
        after: usize,
    ) -> Vec<Effect> {
        let effects = self.open_button_bar_window(Some((bar, Some(after))));
        if let Some(window) = self.button_bar.window.as_mut() {
            window.insert(BarItem::Button(blank_button()));
        }
        effects
    }

    /// The window's keys while it has them: Tab walks the ring, Escape closes, Ctrl+Up and
    /// Ctrl+Down move the item at the cursor, Up and Down no field took move the cursor.
    pub(in crate::app) fn button_bar_window_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        let window = self.button_bar.window.as_ref()?;
        if window.picker.is_some() {
            return self.bar_picker_keys(message);
        }
        let (chord, captured) = match message {
            Message::Key(chord) => (chord, false),
            Message::KeyCaptured(chord) => (chord, true),
            Message::Press => return Some(Vec::new()),
            _ => return None,
        };
        let message = match chord.key.as_str() {
            "Tab" if !chord.ctrl && !chord.alt => {
                return Some(vec![Effect::WalkFields {
                    back: chord.shift,
                    then: None,
                }]);
            }
            "Escape" => WinMsg::Close,
            "ArrowUp" if chord.ctrl => WinMsg::Shift(-1),
            "ArrowDown" if chord.ctrl => WinMsg::Shift(1),
            // An arrow no field took moves the list's cursor.
            "ArrowUp" if !captured => WinMsg::Step(-1),
            "ArrowDown" if !captured => WinMsg::Step(1),
            _ => return None,
        };
        Some(self.on_bar_window(message))
    }

    /// The window as a card over the main window while it has no window of its own.
    pub(in crate::app) fn button_bar_layer(
        &self,
        tokens: &'static crate::theme::Tokens,
    ) -> Option<iced::Element<'_, Message>> {
        let window = self.button_bar.window.as_ref()?;
        Some(layer(window, &self.button_bar.bars, tokens))
    }

    pub(super) fn on_bar_window(&mut self, message: WinMsg) -> Vec<Effect> {
        let bars = self.button_bar.bars.clone();
        let Some(window) = self.button_bar.window.as_mut() else {
            return Vec::new();
        };
        match message {
            WinMsg::Answer(then, answer) => return self.bar_window_answer(then, *answer),
            WinMsg::Close => self.button_bar.window = None,
            WinMsg::Bar(id) => {
                if let Some(bar) = bars.iter().find(|bar| bar.id == id) {
                    *window = BarWindow::of(bar);
                }
            }
            WinMsg::NewBar => *window = BarWindow::default(),
            WinMsg::Name(name) => {
                window.name = name;
                window.dirty = true;
            }
            WinMsg::DeleteBar => window.asking = window.bar.is_some(),
            WinMsg::Confirm(yes) => {
                window.asking = false;
                if let (true, Some(id)) = (yes, window.bar) {
                    return vec![call(self, Then::Deleted, Command::DeleteButtonBar { id })];
                }
            }
            WinMsg::UseAs(place) => {
                let Some(id) = window.bar else {
                    return Vec::new();
                };
                let key = match place {
                    BarPlace::Horizontal => HORIZONTAL_BAR_KEY,
                    BarPlace::Vertical => VERTICAL_BAR_KEY,
                };
                let set = Command::SetSetting {
                    key: key.to_owned(),
                    value: serde_json::Value::from(id),
                };
                return vec![call(self, Then::Used, set)];
            }
            WinMsg::Select(at) => window.cursor = (at < window.items.len()).then_some(at),
            WinMsg::Step(rows) => {
                let last = window.items.len().saturating_sub(1);
                window.cursor = window
                    .cursor
                    .map(|at| at.saturating_add_signed(rows).min(last))
                    .or((!window.items.is_empty()).then_some(0));
            }
            WinMsg::Shift(rows) => {
                if let Some(at) = window.cursor {
                    let to = at.saturating_add_signed(rows);
                    if to < window.items.len() && to != at {
                        window.items.swap(at, to);
                        window.cursor = Some(to);
                        window.dirty = true;
                    }
                }
            }
            WinMsg::AddButton => window.insert(BarItem::Button(blank_button())),
            WinMsg::AddSeparator => window.insert(BarItem::Separator),
            WinMsg::Remove => {
                if let Some(at) = window.cursor.filter(|at| *at < window.items.len()) {
                    window.items.remove(at);
                    window.cursor =
                        (!window.items.is_empty()).then(|| at.min(window.items.len() - 1));
                    window.dirty = true;
                }
            }
            WinMsg::Does(does) => {
                let first = bars
                    .iter()
                    .map(|bar| bar.id)
                    .find(|id| Some(*id) != window.bar);
                if let Some(button) = window.button_mut() {
                    button.action = match (does, first) {
                        (Does::SubBar, Some(id)) => ButtonAction::SubBar(id),
                        (Does::SubBar, None) => return Vec::new(),
                        (Does::Command, _) => ButtonAction::Command(CommandRef::default()),
                    };
                    window.dirty = true;
                }
            }
            WinMsg::Cmd(change) => {
                if let Some(button) = window.button_mut()
                    && let ButtonAction::Command(action) = &mut button.action
                {
                    edit_command(action, change);
                    window.dirty = true;
                }
            }
            WinMsg::SubBar(id) => {
                if let Some(button) = window.button_mut() {
                    button.action = ButtonAction::SubBar(id);
                    window.dirty = true;
                }
            }
            WinMsg::Tooltip(text) => {
                if let Some(button) = window.button_mut() {
                    button.tooltip = text;
                    window.dirty = true;
                }
            }
            WinMsg::IconPath(path) => {
                if let Some(button) = window.button_mut() {
                    let index = button.icon.as_ref().map_or(0, |icon| icon.index);
                    button.icon = Some(rc_engine::IconRef { path, index });
                    window.dirty = true;
                }
            }
            WinMsg::IconIndex(text) => {
                if let Some(button) = window.button_mut()
                    && let Some(icon) = button.icon.as_mut()
                {
                    icon.index = text.trim().parse().unwrap_or(0);
                    window.dirty = true;
                }
            }
            WinMsg::ImportOpen => window.import = Some(Import::default()),
            WinMsg::ImportPath(path) => {
                let import = window.import.get_or_insert_with(Import::default);
                import.path = path;
                import.preview = None;
            }
            WinMsg::ImportRead => {
                if let Some(import) = &window.import {
                    let path = import.path.trim().to_owned();
                    return vec![call(
                        self,
                        Then::Preview,
                        Command::PreviewButtonBarImport { path },
                    )];
                }
            }
            WinMsg::ImportRun => {
                if let Some(import) = window
                    .import
                    .as_ref()
                    .filter(|import| import.preview.is_some())
                {
                    let path = import.path.trim().to_owned();
                    window.busy = true;
                    return vec![call(
                        self,
                        Then::Imported,
                        Command::ImportButtonBar { path },
                    )];
                }
            }
            WinMsg::ImportCancel => window.import = None,
            WinMsg::ChooseSymbol => return self.open_button_symbol_picker(),
            WinMsg::Picker(message) => return self.on_button_symbol_picker(message),
            WinMsg::Save => {
                window.busy = true;
                let save = Command::SaveButtonBar {
                    id: window.bar,
                    name: window.name.clone(),
                    spec: window.spec(),
                };
                return vec![call(self, Then::Saved, save)];
            }
        }
        Vec::new()
    }

    /// The answer to the window's command.
    fn bar_window_answer(
        &mut self,
        then: Then,
        answer: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        let Some(window) = self.button_bar.window.as_mut() else {
            return Vec::new();
        };
        window.busy = false;
        let outcome = match answer {
            Ok(Outcome::ButtonBars(outcome)) => Some(*outcome),
            Ok(_) => None,
            Err(code) => {
                window.note = Some((crate::i18n::error_text(code), true));
                return Vec::new();
            }
        };
        match (then, outcome) {
            (Then::Saved, Some(ButtonBarsOutcome::Saved { id })) => {
                window.bar = Some(id);
                window.dirty = false;
                window.note = Some((crate::i18n::t!("button_bar.saved").to_owned(), false));
            }
            (Then::Deleted, _) => *window = BarWindow::default(),
            (Then::Preview, Some(ButtonBarsOutcome::Preview { preview })) => {
                if let Some(import) = window.import.as_mut() {
                    import.preview = Some(preview);
                }
            }
            (Then::Imported, Some(ButtonBarsOutcome::Imported { bars })) => {
                window.import = None;
                window.dirty = false;
                window.bar = bars.first().copied();
                window.note = Some((
                    crate::i18n::t!("button_bar.imported", count = bars.len()),
                    false,
                ));
            }
            (Then::Used, _) => {
                window.note = Some((crate::i18n::t!("button_bar.used").to_owned(), false));
            }
            _ => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
