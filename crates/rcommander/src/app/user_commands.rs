//! The own commands' window (RC-100-86): Configuration > "Own commands..." (`usercmd.manager`),
//! Start > "Change Start menu..." (`start_menu.manager`, on its Start menu tab) and the command
//! palette open it. Two tabs (`theme::page_tabs`): "Own commands" - the commands `em_<name>` with
//! a filter on the left, the form of the one at the cursor on the right (name, kind, command,
//! parameters, start folder, window, run as administrator, tooltip) - and "Start menu" - the
//! menu as a tree indented as the connection manager's, the form of the row at the cursor (kind,
//! title, an entry's command) and the buttons that add, delete and move rows, into and out of a
//! submenu too. Save and Close below. The keys are `keys.rs`, the lists `list.rs`, the forms
//! `form.rs`, the drawing `view.rs`; the question for a program's parameters (`?`) is `ask.rs`.
//!
//! The window keeps no command of its own: the lists are the engine's (`usercmd.list`,
//! `start_menu.list`, kept in `known.rs`), read at the start and again on every
//! `UserCommandsChanged` and `StartMenuChanged` - also after an agent changed one over MCP -, and
//! every change is a command (`calls.rs`). A new Start menu also fills the menu bar's Start menu
//! again; the palette's rows and the Shortcuts tab read the lists when they draw.

mod ask;
mod calls;
mod form;
mod keys;
pub mod known;
mod list;
mod update;
mod view;

use iced::widget::{Id, operation};
use rc_engine::{Outcome, StartItem, StartItemId, UserCommand, UserCommandId};

pub use self::ask::{ASK_FIELD, Ask, AskMsg};
/// The command model's form, which the internal associations' and the button bars' windows share
/// (RC-100-91, RC-100-87).
pub use self::form::{CmdMsg, OwnMsg, edit, edit as edit_command};
pub use self::view::{FILTER_FIELD, Shown, body, layer};
pub(in crate::app) use self::view::{
    Labeled, choice, command_fields, field, fields, help, labeled,
};
use self::{
    calls::Then,
    form::{OwnForm, StartForm, StartMsg},
    keys::{Focus, KeyAction},
    list::Move,
    view::TREE_STOP_ID,
};
use super::{
    App, Effect, Message,
    dialog_windows::DialogWindow,
    field_walk,
    symbol_picker::{self, PickerMsg, SymbolPicker},
};

/// The window's two tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Own,
    Start,
}

/// The question the window asks besides its forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    /// Delete this own command, or this row of the Start menu (by the tab shown)?
    Delete(i64),
}

/// The open window.
#[derive(Debug, Default)]
pub struct CommandsWindow {
    pub tab: Tab,
    pub filter: String,
    /// The own command at the cursor, whose form is shown unless a new one is typed.
    pub cursor: Option<UserCommandId>,
    pub own: Option<OwnForm>,
    /// The row of the Start menu at the cursor.
    pub start_cursor: Option<StartItemId>,
    pub start: Option<StartForm>,
    pub focus: Focus,
    pub pending: Option<Pending>,
    /// The last outcome: a text, and whether it is an error.
    pub note: Option<(String, bool)>,
    /// A save runs.
    pub busy: bool,
    /// "Choose symbol..." open over the own command's form (RC-110-10).
    pub picker: Option<SymbolPicker>,
}

/// The window and the question for a program's parameters, each while open.
#[derive(Debug, Default)]
pub struct UserCommandsUi {
    pub window: Option<CommandsWindow>,
    pub ask: Option<Ask>,
}

/// What the window's controls and the engine's answers say.
#[derive(Debug, Clone)]
pub enum UcMsg {
    /// `usercmd.list`'s answer.
    Commands(Result<Vec<UserCommand>, &'static str>),
    /// `start_menu.list`'s answer.
    StartMenu(Result<Vec<StartItem>, &'static str>),
    /// The answer to a command.
    Answer(Then, Box<Result<Outcome, &'static str>>),
    Close,
    Tab(Tab),
    Filter(String),
    /// A row of the tab shown clicked.
    Select(i64),
    /// The cursor moved by rows.
    Step(isize),
    /// A new command, or a new row of the Start menu after the cursor.
    New,
    Delete,
    /// The answer to the delete question.
    Confirm(bool),
    /// The Start menu's row at the cursor moved.
    Move(Move),
    Own(OwnMsg),
    Start(StartMsg),
    Save,
    /// "Add key...": the Shortcuts tab with the command or the entry at the cursor chosen.
    AddKey,
    /// After Tab: the id of the stop of the focus ring that has the keyboard.
    Focused(Option<Id>),
    /// The question for a program's parameters (`?`).
    Ask(AskMsg),
    /// "Choose symbol..." beside the own command's symbol (RC-110-10).
    ChooseSymbol,
    Picker(PickerMsg),
}

impl CommandsWindow {
    /// A new list of own commands: the cursor stays on its command (else the first shown), the
    /// form shows it - a form with changes stays as typed.
    pub fn refresh_commands(&mut self, list: &[UserCommand]) {
        self.cursor = list::keep(&list::shown_commands(list, &self.filter), self.cursor);
        let keep = self
            .own
            .as_ref()
            .is_some_and(|form| form.dirty && (form.id.is_none() || form.id == self.cursor));
        if !keep {
            self.select_command(self.cursor, list);
        }
    }

    /// A new Start menu, as [`Self::refresh_commands`].
    pub fn refresh_start(&mut self, list: &[StartItem]) {
        self.start_cursor = list::keep(&list::shown_items(list), self.start_cursor);
        let keep = self
            .start
            .as_ref()
            .is_some_and(|form| form.dirty && (form.id.is_none() || form.id == self.start_cursor));
        if !keep {
            self.select_item(self.start_cursor, list);
        }
    }

    /// The cursor to the own command `id`, its form read as stored.
    pub fn select_command(&mut self, id: Option<UserCommandId>, list: &[UserCommand]) {
        self.cursor = id;
        self.own = id
            .and_then(|id| list.iter().find(|command| command.id == id))
            .map(OwnForm::of);
    }

    /// The cursor to the row `id` of the Start menu, its form read as stored.
    pub fn select_item(&mut self, id: Option<StartItemId>, list: &[StartItem]) {
        self.start_cursor = id;
        self.start = id
            .and_then(|id| list.iter().find(|item| item.id == id))
            .map(StartForm::of);
    }

    /// Whether the form of the tab shown has changes to save.
    pub fn dirty(&self) -> bool {
        match self.tab {
            Tab::Own => self.own.as_ref().is_some_and(|form| form.dirty),
            Tab::Start => self.start.as_ref().is_some_and(|form| form.dirty),
        }
    }
}

/// The stop the keyboard starts on for `tab`: the filter of the own commands, the Start menu's
/// tree.
pub fn first_stop(tab: Tab) -> &'static str {
    match tab {
        Tab::Own => FILTER_FIELD,
        Tab::Start => TREE_STOP_ID,
    }
}

/// The message the ring's stop after a walk turns into.
fn moved(id: Option<Id>) -> Message {
    Message::UserCommands(UcMsg::Focused(id))
}

/// The same while the symbol picker is open.
fn picker_moved(id: Option<Id>) -> Message {
    Message::UserCommands(UcMsg::Picker(PickerMsg::Focused(id)))
}

/// A message of the symbol picker as the window's.
fn picker_message(message: PickerMsg) -> Message {
    Message::UserCommands(UcMsg::Picker(message))
}

fn focus(id: &'static str) -> Effect {
    Effect::Task(operation::focus(Id::new(id)))
}

impl App {
    /// Reads the own commands and the Start menu; at the start and when the window opens.
    pub(in crate::app) fn user_commands_task(&self) -> iced::Task<Message> {
        iced::Task::batch([
            calls::list_commands(self.engine.clone()),
            calls::list_start_menu(self.engine.clone()),
        ])
    }

    /// Opens the window on the Start menu's tab (`start_menu`) or the own commands' - or brings
    /// it to the front on that tab -, the keyboard on the tab's first stop.
    pub(in crate::app) fn open_user_commands(&mut self, start_menu: bool) -> Vec<Effect> {
        let tab = if start_menu { Tab::Start } else { Tab::Own };
        if let Some(window) = self.user_commands.window.as_mut() {
            window.tab = tab;
            window.focus = Focus::first(tab);
            window.pending = None;
            let mut effects = self.dialog_window_front(DialogWindow::Commands);
            effects.push(focus(first_stop(tab)));
            return effects;
        }
        let mut window = CommandsWindow {
            tab,
            focus: Focus::first(tab),
            ..CommandsWindow::default()
        };
        window.refresh_commands(&known::commands());
        window.refresh_start(&known::start_menu());
        self.user_commands.window = Some(window);
        vec![
            focus(first_stop(tab)),
            Effect::Task(self.user_commands_task()),
        ]
    }

    /// The stop the window gives the keyboard when its window shows.
    pub(in crate::app) fn user_commands_focus(&self) -> iced::Task<Message> {
        let tab = self
            .user_commands
            .window
            .as_ref()
            .map_or(Tab::Own, |window| window.tab);
        operation::focus(Id::new(first_stop(tab)))
    }

    /// The window's keys and clicks while it has them; `None` for anything else.
    pub(in crate::app) fn user_commands_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        let window = self.user_commands.window.as_ref()?;
        if let Some(picker) = &window.picker {
            let action = match message {
                Message::Key(chord) => symbol_picker::key(picker.focus, chord, false),
                Message::KeyCaptured(chord) => symbol_picker::key(picker.focus, chord, true),
                Message::Press => return Some(Vec::new()),
                _ => return None,
            };
            return Some(symbol_picker::effects(action, picker_moved, |message| {
                self.on_user_commands(UcMsg::Picker(message))
            }));
        }
        let at = (window.tab, window.focus);
        let pending = window.pending.as_ref();
        let action = match message {
            Message::Key(chord) => keys::key(at, pending, chord, false),
            Message::KeyCaptured(chord) => keys::key(at, pending, chord, true),
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
            KeyAction::Run(message) => self.on_user_commands(message),
        })
    }

    /// The window as a card over the main window while it has no window of its own.
    pub(in crate::app) fn user_commands_layer(
        &self,
        tokens: &'static crate::theme::Tokens,
    ) -> Option<iced::Element<'_, Message>> {
        let window = self.user_commands.window.as_ref()?;
        Some(layer(window, self.user_commands_shown(), tokens))
    }

    /// What the window draws besides its own state: the lists and the key of the form's command
    /// or entry.
    pub(in crate::app) fn user_commands_shown(&self) -> Shown {
        let name = self.user_commands.window.as_ref().and_then(view::key_name);
        let icon_size = self.button_bar.settings.icon_size;
        let icon = self
            .own_symbol_key()
            .and_then(|key| self.button_bar.symbols.get(&key).cloned());
        Shown {
            commands: known::commands(),
            items: known::start_menu(),
            key: name.and_then(|name| super::palette::menu_bar::key_of(&self.keymap, &name)),
            icon,
            icon_size,
        }
    }

    /// The symbol of the own command in the form, at the bars' size.
    fn own_symbol_key(&self) -> Option<super::button_bar::SymbolKey> {
        let form = self.user_commands.window.as_ref()?.own.as_ref()?;
        form.symbol_key(self.button_bar.settings.icon_size)
    }

    /// Reads the form's symbol off the window's thread, once: the button bar's reading and its
    /// store of what it read (RC-100-87).
    pub(in crate::app) fn own_symbol(&mut self) -> Vec<Effect> {
        let Some(key) = self.own_symbol_key() else {
            return Vec::new();
        };
        match self.button_bar.symbols.ask(vec![key]) {
            Some(task) => vec![Effect::Task(task)],
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "user_commands_tests.rs"]
mod tests;
