//! The window's messages (RC-100-86): the tabs, the list's and the tree's keys and buttons, the
//! delete question, the forms, and the engine's answers - each change a command (`calls.rs`), the
//! lists read again after it. Also what the engine asks of the window for a running own command:
//! a row only the window runs (`WindowRow`).

use rc_engine::{Command, Outcome, StartItem, UserCommand};

use super::{
    CommandsWindow, Pending, Tab, UcMsg, calls,
    calls::Then,
    focus,
    form::{OwnForm, StartForm},
    keys::{self, Focus},
    known, list,
    view::{self, NAME_FIELD, TITLE_FIELD},
};
use crate::{
    app::{
        App, Effect, Message,
        settings::{CommandKeyMsg, Page, SettingsMessage, ShortcutsMsg, Tab as SettingsTab},
        symbol_picker::{self, Outcome as Picked, PickerMsg},
    },
    i18n::{error_text, t},
};

impl App {
    /// A message of the window or of the question for a program's parameters.
    pub(in crate::app) fn on_user_commands(&mut self, message: UcMsg) -> Vec<Effect> {
        match message {
            UcMsg::Commands(Ok(list)) => {
                let mut effects = self.commands_listed(list);
                // A button bar's button of an own command shows that command's symbol (RC-100-87).
                effects.extend(self.button_bar_symbols());
                return effects;
            }
            UcMsg::StartMenu(Ok(list)) => return self.start_menu_listed(list),
            UcMsg::Commands(Err(code)) | UcMsg::StartMenu(Err(code)) => {
                tracing::warn!(code, "the own commands or the Start menu were not read");
                if let Some(window) = self.user_commands.window.as_mut() {
                    window.note = Some((error_text(code), true));
                }
                return Vec::new();
            }
            UcMsg::Answer(then, answer) => return self.user_commands_answer(then, *answer),
            UcMsg::Close => {
                self.user_commands.window = None;
                return Vec::new();
            }
            UcMsg::AddKey => return self.user_command_key(),
            UcMsg::Ask(message) => return self.on_ask(message),
            UcMsg::Save => return self.save_user_command(),
            UcMsg::ChooseSymbol => return self.open_own_symbol_picker(),
            UcMsg::Picker(message) => return self.on_own_symbol_picker(message),
            _ => {}
        }
        let Some(window) = self.user_commands.window.as_mut() else {
            return Vec::new();
        };
        let commands = known::commands();
        let items = known::start_menu();
        match message {
            UcMsg::Tab(tab) => {
                window.tab = tab;
                window.pending = None;
                window.note = None;
            }
            UcMsg::Filter(filter) => {
                window.filter = filter;
                let shown = list::shown_commands(&commands, &window.filter);
                let cursor = list::keep(&shown, window.cursor);
                if cursor != window.cursor {
                    window.select_command(cursor, &commands);
                }
            }
            UcMsg::Select(id) => {
                select(window, Some(id), &commands, &items);
                window.focus = Focus::List;
            }
            UcMsg::Step(by) => {
                let (shown, cursor) = match window.tab {
                    Tab::Own => (
                        list::shown_commands(&commands, &window.filter),
                        window.cursor,
                    ),
                    Tab::Start => (list::shown_items(&items), window.start_cursor),
                };
                let next = list::step(&shown, cursor, by);
                if next != cursor {
                    select(window, next, &commands, &items);
                }
            }
            UcMsg::New => {
                window.pending = None;
                return match window.tab {
                    Tab::Own => {
                        window.own = Some(OwnForm::blank());
                        vec![focus(NAME_FIELD)]
                    }
                    Tab::Start => {
                        let place = list::new_place(&items, window.start_cursor);
                        window.start = Some(StartForm::new(place));
                        vec![focus(TITLE_FIELD)]
                    }
                };
            }
            UcMsg::Delete => delete(window, &commands, &items),
            UcMsg::Confirm(yes) => {
                let asked = window.pending.take();
                if let (true, Some(Pending::Delete(id))) = (yes, asked) {
                    let command = match window.tab {
                        Tab::Own => Command::DeleteUserCommand { id },
                        Tab::Start => Command::DeleteStartItem { id },
                    };
                    let engine = self.engine.clone();
                    return vec![Effect::Task(calls::run(engine, Then::Changed, command))];
                }
            }
            UcMsg::Move(to) => {
                let place = window
                    .start_cursor
                    .filter(|_| window.tab == Tab::Start)
                    .and_then(|id| list::moved(&items, id, to).map(|place| (id, place)));
                if let Some((id, place)) = place {
                    let command = Command::MoveStartItem {
                        id,
                        parent: place.parent,
                        position: place.position,
                    };
                    let engine = self.engine.clone();
                    return vec![Effect::Task(calls::run(engine, Then::Changed, command))];
                }
            }
            UcMsg::Own(message) => {
                if let Some(form) = window.own.as_mut() {
                    form.update(message);
                }
            }
            UcMsg::Start(message) => {
                if let Some(form) = window.start.as_mut() {
                    form.update(message);
                }
            }
            UcMsg::Focused(id) => window.focus = keys::focus_of(id.as_ref()),
            UcMsg::Commands(_)
            | UcMsg::StartMenu(_)
            | UcMsg::Answer(..)
            | UcMsg::Close
            | UcMsg::AddKey
            | UcMsg::Ask(_)
            | UcMsg::Save
            | UcMsg::ChooseSymbol
            | UcMsg::Picker(_) => {}
        }
        // The form's symbol, read once when the form shows a new one.
        self.own_symbol()
    }

    /// "Choose symbol...": the picker over the form, on the form's symbol (RC-110-10).
    fn open_own_symbol_picker(&mut self) -> Vec<Effect> {
        let Some(form) = self
            .user_commands
            .window
            .as_ref()
            .and_then(|window| window.own.as_ref())
        else {
            return Vec::new();
        };
        let picker = self.symbol_picker(&form.icon_path, &form.tooltip);
        let follow = symbol_picker::follow(&picker);
        if let Some(window) = self.user_commands.window.as_mut() {
            window.picker = Some(picker);
            window.pending = None;
        }
        vec![symbol_picker::focus_search(), follow]
    }

    /// A message of the picker: the symbol taken into the form, or the form back as it was.
    fn on_own_symbol_picker(&mut self, message: PickerMsg) -> Vec<Effect> {
        let Some(window) = self.user_commands.window.as_mut() else {
            return Vec::new();
        };
        let Some(picker) = window.picker.as_mut() else {
            return Vec::new();
        };
        match picker.update(message) {
            Picked::Open(true) => return vec![symbol_picker::follow(picker)],
            Picked::Open(false) => return Vec::new(),
            Picked::Chosen(reference) => {
                if let Some(form) = window.own.as_mut() {
                    form.choose_symbol(reference);
                }
            }
            Picked::Cancelled => {}
        }
        window.picker = None;
        window.focus = Focus::Ring;
        vec![focus(NAME_FIELD)]
    }

    /// A new list of own commands: kept for the palette and the Shortcuts tab, the open window
    /// refreshed.
    fn commands_listed(&mut self, list: Vec<UserCommand>) -> Vec<Effect> {
        if let Some(window) = self.user_commands.window.as_mut() {
            window.refresh_commands(&list);
        }
        known::set_commands(list);
        self.own_symbol()
    }

    /// A new Start menu: kept, the open window refreshed, the menu bar's Start menu made again.
    fn start_menu_listed(&mut self, list: Vec<StartItem>) -> Vec<Effect> {
        if let Some(window) = self.user_commands.window.as_mut() {
            window.refresh_start(&list);
        }
        known::set_start_menu(list);
        vec![Effect::Task(self.refill_menu_bar())]
    }

    /// Save, or Enter in the list: the form of the tab shown stored (`usercmd.save`,
    /// `start_menu.save`), when it has changes.
    fn save_user_command(&mut self) -> Vec<Effect> {
        let Some(window) = self.user_commands.window.as_mut() else {
            return Vec::new();
        };
        if window.busy || !window.dirty() {
            return Vec::new();
        }
        let (then, command) = match window.tab {
            Tab::Own => {
                let Some(form) = window.own.as_ref() else {
                    return Vec::new();
                };
                let command = Command::SaveUserCommand {
                    id: form.id,
                    command: form.draft(),
                };
                (Then::OwnSaved, command)
            }
            Tab::Start => {
                let Some(form) = window.start.as_ref() else {
                    return Vec::new();
                };
                let command = Command::SaveStartItem {
                    id: form.id,
                    parent: form.place.parent,
                    item: form.draft(),
                };
                let new = form.id.is_none().then_some(form.place);
                (Then::StartSaved { new }, command)
            }
        };
        window.busy = true;
        vec![Effect::Task(calls::run(self.engine.clone(), then, command))]
    }

    /// The answer to a command of the window: a refusal in the note; a saved form clean, the
    /// cursor on what was saved - a new row of the Start menu moved to its place then -; the
    /// lists read again.
    fn user_commands_answer(
        &mut self,
        then: Then,
        answer: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        let Some(window) = self.user_commands.window.as_mut() else {
            return Vec::new();
        };
        window.busy = false;
        let outcome = match answer {
            Ok(outcome) => outcome,
            Err(code) => {
                window.note = Some((error_text(code), true));
                return Vec::new();
            }
        };
        window.note = None;
        let id = calls::saved_id(&outcome);
        let mut effects = Vec::new();
        match then {
            Then::OwnSaved => {
                if let Some(form) = window.own.as_mut() {
                    if form.id.is_none() {
                        window.filter.clear();
                    }
                    form.dirty = false;
                    form.id = id.or(form.id);
                }
                window.cursor = id.or(window.cursor);
                window.note = Some((t!("user_commands.saved").to_owned(), false));
            }
            Then::StartSaved { new } => {
                if let Some(form) = window.start.as_mut() {
                    form.dirty = false;
                    form.id = id.or(form.id);
                }
                window.start_cursor = id.or(window.start_cursor);
                window.note = Some((t!("user_commands.saved").to_owned(), false));
                if let (Some(id), Some(place)) = (id, new)
                    && place.position.is_some()
                {
                    let command = Command::MoveStartItem {
                        id,
                        parent: place.parent,
                        position: place.position,
                    };
                    let engine = self.engine.clone();
                    effects.push(Effect::Task(calls::run(engine, Then::Changed, command)));
                }
            }
            Then::Changed => {}
        }
        // The engine announces the change as well; this read comes after the answer for sure.
        effects.push(Effect::Task(self.user_commands_task()));
        effects
    }

    /// "Add key...": the window closes and the Shortcuts tab opens with the command or the
    /// entry at the cursor chosen for its key - the settings draw over every dialog.
    fn user_command_key(&mut self) -> Vec<Effect> {
        let Some(name) = self.user_commands.window.as_ref().and_then(view::key_name) else {
            return Vec::new();
        };
        self.user_commands.window = None;
        let settings = Message::Settings;
        let mut effects = self.apply(settings(SettingsMessage::OpenPage(Page::Keyboard)));
        effects.extend(self.apply(settings(SettingsMessage::Tab(SettingsTab::Shortcuts))));
        let open = ShortcutsMsg::Command(CommandKeyMsg::OpenFor(name));
        effects.extend(self.apply(settings(SettingsMessage::Shortcuts(open))));
        effects
    }

    /// A row of an own command that only the window runs (a `"window"` row of the keymap, such
    /// as the command line's): run as its key would be; a row without a key says so.
    pub(in crate::app) fn window_row(&mut self, name: &str) -> Vec<Effect> {
        let chord = self
            .keymap
            .window()
            .iter()
            .find(|key| key.name == name)
            .map(|key| key.chord.clone());
        match chord {
            Some(chord) => self.on_key(chord, false),
            None => {
                self.info = Some(t!("user_commands.window_row_unbound", name = name));
                Vec::new()
            }
        }
    }
}

/// The cursor of the tab shown to `id`, its form read as stored.
fn select(
    window: &mut CommandsWindow,
    id: Option<i64>,
    commands: &[UserCommand],
    items: &[StartItem],
) {
    match window.tab {
        Tab::Own => window.select_command(id, commands),
        Tab::Start => window.select_item(id, items),
    }
}

/// Delete or F8: a new form not yet saved is dropped; else the question for the row at the
/// cursor.
fn delete(window: &mut CommandsWindow, commands: &[UserCommand], items: &[StartItem]) {
    let (new, cursor) = match window.tab {
        Tab::Own => (
            window.own.as_ref().is_some_and(|form| form.id.is_none()),
            window.cursor,
        ),
        Tab::Start => (
            window.start.as_ref().is_some_and(|form| form.id.is_none()),
            window.start_cursor,
        ),
    };
    if new {
        select(window, cursor, commands, items);
    } else {
        window.pending = cursor.map(Pending::Delete);
    }
}
