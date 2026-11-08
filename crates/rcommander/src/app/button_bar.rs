//! The button bars (RC-100-87), as Total Commander has them: a bar of buttons under the menu bar
//! (`appearance.button_bar`, on at the first start with the bar "Default") and a second,
//! vertical one between the panels (`appearance.vertical_button_bar`, off), each showing the bar
//! its setting names (`buttonbar.horizontal`, `buttonbar.vertical`; 0 or a bar gone: the first).
//! A button runs its command for the active panel (`button_bar.run`), or opens its sub-bar as a
//! menu under it (`menu.rs`); a right click or Shift+F10 opens the button's own menu - Change,
//! Delete, Insert button, Edit bar. Alt+Shift+F11 and Alt+Shift+F12 give a bar the keyboard
//! (`keys.rs`). Files dropped on a button that starts a program are handed to it, Shift and a
//! drop on a bar make a new button of the file (`drop.rs`). The symbols are read off the window's
//! thread (`symbols.rs`); a button that runs rCommander's commands without one shows a glyph of
//! the app's icon font (`glyphs.rs`), and only a button with no icon at all shows text.
//! The bars are edited in their window (`window.rs`, `DialogWindow::ButtonBar`).
//!
//! The window keeps no bar of its own: the list is the engine's (`button_bar.list`), read at the
//! start and again after every `ButtonBarsChanged`; the settings are read at the start and after
//! a change of one of them, which also shows or hides the drive bar, the command line and the
//! status bar (Configuration > Layout in Total Commander, `view.toggle_bar` from the View menu).

mod drop;
mod glyphs;
mod keys;
mod menu;
mod symbols;
mod view;
mod window;

use iced::{Rectangle, Task};
use rc_engine::{
    BarItem, BarPlace, ButtonAction, ButtonBar, ButtonBarId, ButtonBarsOutcome, Command, Engine,
    EngineEvent, Outcome,
    settings::{
        BUTTON_BAR_KEY, COMMAND_LINE_KEY, DRIVE_BAR_KEY, HORIZONTAL_BAR_KEY, ICON_SIZE_KEY,
        STATUS_BAR_KEY, VERTICAL_BAR_KEY, VERTICAL_BUTTON_BAR_KEY, stored_bool, stored_setting,
    },
};

pub use self::drop::Target;
pub use self::menu::{MenuKind, MenuMsg, OpenMenu};
pub use self::symbols::SymbolKey;
pub(super) use self::view::{horizontal, vertical, with_menu};
#[cfg(test)]
pub use self::window::Import;
pub use self::window::{BarWindow, NAME_FIELD, WinMsg, body as window_body};
use super::{App, Effect, Message, icons::Rgba};

/// The settings the bars read, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarSettings {
    /// `appearance.button_bar`.
    pub horizontal: bool,
    /// `appearance.vertical_button_bar`.
    pub vertical: bool,
    /// `buttonbar.horizontal`: the bar shown above the panels.
    pub horizontal_bar: ButtonBarId,
    /// `buttonbar.vertical`: the bar shown between them.
    pub vertical_bar: ButtonBarId,
    /// `buttonbar.icon_size`, in pixels.
    pub icon_size: u16,
    /// `appearance.drive_bar`.
    pub drive_bar: bool,
    /// `appearance.command_line`.
    pub command_line: bool,
    /// `appearance.status_bar`.
    pub status_bar: bool,
}

impl Default for BarSettings {
    /// The schema's defaults: the button bar on, the vertical one off, every other bar shown.
    fn default() -> Self {
        Self {
            horizontal: true,
            vertical: false,
            horizontal_bar: 0,
            vertical_bar: 0,
            icon_size: 24,
            drive_bar: true,
            command_line: true,
            status_bar: true,
        }
    }
}

/// The keys of the settings the bars read.
const KEYS: [&str; 8] = [
    BUTTON_BAR_KEY,
    VERTICAL_BUTTON_BAR_KEY,
    HORIZONTAL_BAR_KEY,
    VERTICAL_BAR_KEY,
    ICON_SIZE_KEY,
    DRIVE_BAR_KEY,
    COMMAND_LINE_KEY,
    STATUS_BAR_KEY,
];

/// Whether a change of the setting `key` concerns the bars.
pub fn concerns(key: &str) -> bool {
    KEYS.contains(&key)
}

/// Where the keyboard is in a bar: its place and the item at the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarFocus {
    pub place: BarPlace,
    pub index: usize,
}

/// The bars, the keyboard in them, the open menu, the symbols and the window.
#[derive(Debug, Default)]
pub struct ButtonBarUi {
    pub bars: Vec<ButtonBar>,
    pub settings: BarSettings,
    pub focus: Option<BarFocus>,
    pub menu: Option<OpenMenu>,
    pub symbols: symbols::Symbols,
    pub window: Option<BarWindow>,
}

impl ButtonBarUi {
    /// The bar shown at `place`: the one its setting names, else the first; none while the place
    /// is switched off.
    pub fn shown(&self, place: BarPlace) -> Option<&ButtonBar> {
        let (on, id) = match place {
            BarPlace::Horizontal => (self.settings.horizontal, self.settings.horizontal_bar),
            BarPlace::Vertical => (self.settings.vertical, self.settings.vertical_bar),
        };
        if !on {
            return None;
        }
        self.bars
            .iter()
            .find(|bar| bar.id == id)
            .or_else(|| self.bars.first())
    }

    /// The bar `id`.
    pub fn bar(&self, id: ButtonBarId) -> Option<&ButtonBar> {
        self.bars.iter().find(|bar| bar.id == id)
    }
}

/// What the bars' controls, the menu, the window and the engine's answers say.
#[derive(Debug, Clone)]
pub enum BbMsg {
    /// `button_bar.list`'s answer.
    Bars(Result<Vec<ButtonBar>, &'static str>),
    /// The settings, as stored.
    Settings(BarSettings),
    /// Symbols read: each key with its pixels, `None` for one that did not read.
    Symbols(Vec<(SymbolKey, Option<Rgba>)>),
    /// A click on the item at this place of the bar.
    Press(BarPlace, usize),
    /// A right click on it: the button's menu.
    Context(BarPlace, usize),
    /// `»`: the items from this one on, which the bar has no room for, as a menu.
    More(BarPlace, usize),
    /// The button the menu opens under was measured.
    Anchored(Option<Rectangle>),
    Menu(MenuMsg),
    /// A drop measured: the bar or button under the pointer, with Shift held or not.
    Dropped {
        paths: Vec<std::path::PathBuf>,
        target: Option<Target>,
        shift: bool,
        pointer: Option<crate::platform::drag_out::Pointer>,
    },
    Window(WinMsg),
}

/// `message` as the app's.
pub fn wrap(message: BbMsg) -> Message {
    Message::ButtonBar(message)
}

/// `button_bar.list`, its answer as [`BbMsg::Bars`].
fn list(engine: Engine) -> Task<Message> {
    Task::perform(
        async move {
            match engine.dispatch(Command::ListButtonBars {}).await {
                Ok(Outcome::ButtonBars(outcome)) => match *outcome {
                    ButtonBarsOutcome::Bars { bars } => Ok(bars),
                    _ => Err("database_error"),
                },
                Ok(_) => Err("database_error"),
                Err(error) => Err(error.code()),
            }
        },
        |listed| wrap(BbMsg::Bars(listed)),
    )
}

impl App {
    /// Reads the bars and their settings; at the start and when the bars changed.
    pub(in crate::app) fn button_bar_task(&self) -> Task<Message> {
        Task::batch([list(self.engine.clone()), self.button_bar_settings_task()])
    }

    /// Reads the bars' settings; what is not stored keeps its default.
    pub(in crate::app) fn button_bar_settings_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let id = |value: Option<serde_json::Value>| {
                    value.and_then(|value| value.as_i64()).unwrap_or(0)
                };
                let size = stored_setting(&db, ICON_SIZE_KEY)
                    .await
                    .and_then(|value| value.as_str().and_then(|text| text.parse::<u16>().ok()))
                    .unwrap_or(24);
                BarSettings {
                    horizontal: stored_bool(&db, BUTTON_BAR_KEY).await,
                    vertical: stored_bool(&db, VERTICAL_BUTTON_BAR_KEY).await,
                    horizontal_bar: id(stored_setting(&db, HORIZONTAL_BAR_KEY).await),
                    vertical_bar: id(stored_setting(&db, VERTICAL_BAR_KEY).await),
                    icon_size: size,
                    drive_bar: stored_bool(&db, DRIVE_BAR_KEY).await,
                    command_line: stored_bool(&db, COMMAND_LINE_KEY).await,
                    status_bar: stored_bool(&db, STATUS_BAR_KEY).await,
                }
            },
            |settings| wrap(BbMsg::Settings(settings)),
        )
    }

    /// The engine's events of the bars: read again, their window, the keyboard into a bar.
    pub(in crate::app) fn button_bar_event(&mut self, event: &EngineEvent) -> Vec<Effect> {
        match event {
            EngineEvent::ButtonBarsChanged => vec![Effect::Task(list(self.engine.clone()))],
            EngineEvent::ButtonBarWindow => self.open_button_bar_window(None),
            EngineEvent::ButtonBarFocus { bar } => self.focus_bar(*bar),
            _ => Vec::new(),
        }
    }

    /// The open menu's keys, and the keys of the bar that has the keyboard (Alt+Shift+F11,
    /// Alt+Shift+F12): every key stays there; a click elsewhere takes the keyboard back to the
    /// panels. `None` while neither is open.
    pub(in crate::app) fn button_bar_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        let chord = match message {
            Message::Key(chord) | Message::KeyCaptured(chord) => chord,
            Message::Press => {
                self.button_bar.focus = None;
                return None;
            }
            _ => return None,
        };
        if self.button_bar.menu.is_some() {
            return Some(self.on_bar_menu(MenuMsg::Key(chord.clone())));
        }
        let focus = self.button_bar.focus?;
        Some(self.bar_key(focus, chord))
    }

    /// The keyboard to the bar at `place`, on its first button; nothing while it does not show.
    fn focus_bar(&mut self, place: BarPlace) -> Vec<Effect> {
        let ui = &mut self.button_bar;
        let Some(bar) = ui.shown(place) else {
            return Vec::new();
        };
        let Some(index) = keys::first(&bar.spec.items) else {
            return Vec::new();
        };
        ui.menu = None;
        ui.focus = Some(BarFocus { place, index });
        Vec::new()
    }

    pub(in crate::app) fn on_button_bar(&mut self, message: BbMsg) -> Vec<Effect> {
        match message {
            BbMsg::Bars(Ok(bars)) => {
                self.button_bar.bars = bars;
                self.button_bar_refresh();
                if let Some(window) = self.button_bar.window.as_mut() {
                    window.refresh(&self.button_bar.bars);
                }
                self.button_bar_symbols()
            }
            BbMsg::Bars(Err(code)) => {
                tracing::warn!(code, "the button bars did not read");
                Vec::new()
            }
            BbMsg::Settings(settings) => {
                self.button_bar.settings = settings;
                self.button_bar_refresh();
                self.button_bar_symbols()
            }
            BbMsg::Symbols(read) => {
                self.button_bar.symbols.loaded(read);
                Vec::new()
            }
            BbMsg::Press(place, index) => {
                self.button_bar.focus = None;
                self.press_bar_item(place, index)
            }
            BbMsg::Context(place, index) => self.open_button_menu(place, index),
            BbMsg::More(place, from) => self.open_more(place, from),
            BbMsg::Anchored(bounds) => {
                if let Some(menu) = self.button_bar.menu.as_mut() {
                    menu.anchor(bounds);
                }
                Vec::new()
            }
            BbMsg::Menu(message) => self.on_bar_menu(message),
            BbMsg::Dropped {
                paths,
                target,
                shift,
                pointer,
            } => self.dropped_on_bar(paths, target, shift, pointer),
            BbMsg::Window(message) => self.on_bar_window(message),
        }
    }

    /// After new bars or settings: a keyboard cursor or a menu on a bar that went is dropped.
    fn button_bar_refresh(&mut self) {
        let ui = &mut self.button_bar;
        if let Some(focus) = ui.focus {
            let fits = ui
                .shown(focus.place)
                .is_some_and(|bar| bar.spec.items.get(focus.index).is_some());
            if !fits {
                ui.focus = None;
            }
        }
        if let Some(menu) = &ui.menu
            && ui.bar(menu.kind.bar()).is_none()
        {
            ui.menu = None;
        }
    }

    /// The item at `index` of the bar at `place`: a command runs for the active panel, a sub-bar
    /// opens as a menu under it.
    fn press_bar_item(&mut self, place: BarPlace, index: usize) -> Vec<Effect> {
        let Some(bar) = self.button_bar.shown(place) else {
            return Vec::new();
        };
        let id = bar.id;
        match bar.spec.items.get(index) {
            Some(BarItem::Button(button)) => match &button.action {
                ButtonAction::Command(_) => {
                    self.button_bar.menu = None;
                    vec![self.run_button(id, index, Vec::new())]
                }
                ButtonAction::SubBar(sub) => {
                    let sub = *sub;
                    self.open_sub_bar(place, index, sub)
                }
            },
            _ => Vec::new(),
        }
    }

    /// `button_bar.run` of button `index` of bar `bar` for the active panel, with `files`.
    fn run_button(&self, bar: ButtonBarId, index: usize, files: Vec<String>) -> Effect {
        Effect::Dispatch(
            self.active,
            vec![Command::RunButton {
                bar,
                button: index,
                panel: Some(self.active),
                params: None,
                files,
            }],
        )
    }

    /// The symbols of every bar's buttons - the bars shown and the sub-bars their menus open -
    /// not yet asked for, read off the window's thread; also after the own commands are read,
    /// whose symbols their buttons show.
    pub(in crate::app) fn button_bar_symbols(&mut self) -> Vec<Effect> {
        let size = self.button_bar.settings.icon_size;
        let commands = super::user_commands::known::commands();
        let wanted: Vec<SymbolKey> = self
            .button_bar
            .bars
            .iter()
            .flat_map(|bar| bar.spec.items.iter())
            .filter_map(BarItem::button)
            .filter_map(|button| symbols::key_among(button, size, &commands))
            .collect();
        match self.button_bar.symbols.ask(wanted) {
            Some(task) => vec![Effect::Task(task)],
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "button_bar_tests.rs"]
mod tests;
