//! The context menu of a bar (RC-110-15), as Total Commander's: a right click on a button, or
//! beside the buttons, and Shift+F10 or the context-menu key while the bar has the keyboard
//! (Alt+Shift+F11, Alt+Shift+F12) open it. On a button it offers Change..., Delete and Insert
//! button... (TC's Change and Delete, rCommander's insert of RC-100-87); everywhere Edit bar...,
//! Symbol size with 16, 24, 32 and 48 pixels - the sizes of TC's own symbols - and Other..., which
//! opens Settings > Appearance > Window, Flat buttons with its check mark, and Hide bar.
//!
//! It is the file list's own menu (`app/context_menu/`, as the Themes tab's): the same width and
//! card, the arrows, Home and End, a letter takes or moves (`menu_letter`), Enter or Space take
//! the entry, Escape, Shift+F10 and the menu key close it; every other key is swallowed. Symbol
//! size opens its sizes in the same card - Enter, Space or Right -, and Escape or Left go back.
//! The menu stands under the button (beside it in the vertical bar), or under the bar's start
//! for a click beside the buttons, kept inside the bar's length ([`spot`]). Drawn by
//! `context_view.rs`. Each entry is reached by the menu's own keys, as in every menu rCommander
//! draws; the bar itself is reached by its keys.

use iced::{
    Point, Rectangle, Task,
    advanced::widget::{Id, Operation, operate, operation::Outcome},
};
use rc_engine::{
    BarKind, BarPlace, ButtonBarId, Chord, Command,
    settings::{FLAT_KEY, ICON_SIZE_KEY},
};
use serde_json::Value;

use super::{App, BarSettings, BbMsg, Effect, view, wrap};
use crate::{
    app::{
        Message,
        context_menu::{MENU_PADDING, MENU_WIDTH},
        menu_letter::{self, Letter},
        settings::{Page, SettingsMessage, Tab},
    },
    i18n::t,
    theme,
};

/// The symbol sizes the menu offers: those Total Commander's own symbols come in.
pub const SIZES: [u16; 4] = [16, 24, 32, 48];
/// How far from the bar's start the menu stands for a click beside the buttons.
const INDENT: f32 = 8.0;
/// The gap between the button or the bar and the menu.
const GAP: f32 = 2.0;

/// What an entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// The button in the bars' window.
    Change,
    /// The button out of the bar.
    Delete,
    /// A new button after it, in the bars' window.
    Insert,
    /// The bar in the bars' window.
    EditBar,
    /// The sizes below.
    IconSize,
    /// `buttonbar.flat` turned over.
    Flat,
    /// The bar hidden.
    Hide,
    /// `buttonbar.icon_size` set.
    Size(u16),
    /// Settings > Appearance > Window, where any size from 16 to 48 is typed.
    OtherSize,
}

/// Which entries the card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Main,
    Sizes,
}

/// One entry as the card shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub entry: Entry,
    pub label: String,
    /// The check mark: flat buttons on, the size in force.
    pub checked: bool,
    /// It opens more entries (`>`).
    pub opens: bool,
}

/// The open menu.
#[derive(Debug, Clone, PartialEq)]
pub struct BarMenu {
    /// The bar it was opened on.
    pub place: BarPlace,
    pub bar: ButtonBarId,
    /// The button it was opened for; `None` beside the buttons.
    pub button: Option<usize>,
    pub level: Level,
    /// The entry the keyboard or the pointer is on.
    pub cursor: usize,
    /// Its top-left corner in the window; `None` until measured.
    pub at: Option<Point>,
}

/// What the menu's entries and its layer say.
#[derive(Debug, Clone, PartialEq)]
pub enum CtxMsg {
    /// Where the menu stands; `None` when the bar is not shown.
    Placed(Option<Point>),
    /// The pointer is over this entry.
    Hover(usize),
    /// A click on this entry.
    Pick(usize),
    /// A click beside the menu.
    Close,
}

/// What a key does to the open menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    /// It stays open; the cursor or the level may have changed.
    Stay,
    Close,
    /// This entry runs.
    Run(usize),
}

/// The entries of `level` for a menu opened on a button (`button`) or beside the buttons.
pub fn entries(button: bool, level: Level) -> Vec<Entry> {
    match level {
        Level::Main => {
            let own: &[Entry] = if button {
                &[Entry::Change, Entry::Delete, Entry::Insert]
            } else {
                &[]
            };
            own.iter()
                .copied()
                .chain([Entry::EditBar, Entry::IconSize, Entry::Flat, Entry::Hide])
                .collect()
        }
        Level::Sizes => SIZES
            .iter()
            .map(|&size| Entry::Size(size))
            .chain([Entry::OtherSize])
            .collect(),
    }
}

/// The words of `entry` in the bar at `place`.
fn label(entry: Entry, place: BarPlace) -> String {
    match entry {
        Entry::Change => t!("button_bar.menu.change").to_owned(),
        Entry::Delete => t!("button_bar.menu.delete").to_owned(),
        Entry::Insert => t!("button_bar.menu.insert").to_owned(),
        Entry::EditBar => t!("button_bar.menu.edit_bar").to_owned(),
        Entry::IconSize => t!("button_bar.menu.icon_size").to_owned(),
        Entry::Flat => t!("button_bar.menu.flat").to_owned(),
        Entry::Hide => match place {
            BarPlace::Horizontal => t!("button_bar.menu.hide").to_owned(),
            BarPlace::Vertical => t!("button_bar.menu.hide_vertical").to_owned(),
        },
        Entry::Size(size) => t!("button_bar.menu.size", size = size),
        Entry::OtherSize => t!("button_bar.menu.other_size").to_owned(),
    }
}

/// The entries of `level` as the card shows them, with the marks `settings` give.
pub fn items(place: BarPlace, button: bool, level: Level, settings: &BarSettings) -> Vec<Item> {
    entries(button, level)
        .into_iter()
        .map(|entry| Item {
            entry,
            label: label(entry, place),
            checked: match entry {
                Entry::Flat => settings.flat,
                Entry::Size(size) => settings.icon_size == size,
                Entry::OtherSize => !SIZES.contains(&settings.icon_size),
                _ => false,
            },
            opens: entry == Entry::IconSize,
        })
        .collect()
}

/// Shift+F10 or the context-menu key.
pub fn is_menu_key(chord: &Chord) -> bool {
    match chord.key.as_str() {
        "F10" => chord.shift && !chord.ctrl && !chord.alt,
        "ContextMenu" => chord.is_plain(),
        _ => false,
    }
}

impl BarMenu {
    pub fn new(place: BarPlace, bar: ButtonBarId, button: Option<usize>) -> Self {
        Self {
            place,
            bar,
            button,
            level: Level::Main,
            cursor: 0,
            at: None,
        }
    }

    /// The entries the card shows now.
    pub fn items(&self, settings: &BarSettings) -> Vec<Item> {
        items(self.place, self.button.is_some(), self.level, settings)
    }

    /// The sizes, on the one in force, else on Other....
    pub fn open_sizes(&mut self, settings: &BarSettings) {
        self.level = Level::Sizes;
        self.cursor = SIZES
            .iter()
            .position(|&size| size == settings.icon_size)
            .unwrap_or(SIZES.len());
    }

    /// Back from the sizes, on Symbol size.
    fn back(&mut self) {
        self.level = Level::Main;
        self.cursor = entries(self.button.is_some(), Level::Main)
            .iter()
            .position(|entry| *entry == Entry::IconSize)
            .unwrap_or(0);
    }

    /// A key while the menu is open, as the file list's menu takes it; `items` what it shows.
    pub fn key(&mut self, chord: &Chord, items: &[Item]) -> MenuKey {
        if menu_letter::is_letter(chord) {
            let labels = items.iter().map(|item| Some(item.label.as_str()));
            return match menu_letter::letter(&chord.key, labels, self.cursor) {
                Letter::None => MenuKey::Stay,
                Letter::Move(at) => {
                    self.cursor = at;
                    MenuKey::Stay
                }
                Letter::Take(at) => MenuKey::Run(at),
            };
        }
        if is_menu_key(chord) {
            return MenuKey::Close;
        }
        let plain = chord.is_plain();
        let count = items.len().max(1);
        let sizes = self.level == Level::Sizes;
        match chord.key.as_str() {
            "Escape" if sizes => self.back(),
            "Escape" => return MenuKey::Close,
            "ArrowLeft" if plain && sizes => self.back(),
            "ArrowRight" if plain => {
                if items.get(self.cursor).is_some_and(|item| item.opens) {
                    return MenuKey::Run(self.cursor);
                }
            }
            "ArrowUp" if plain => self.cursor = (self.cursor + count - 1) % count,
            "ArrowDown" if plain => self.cursor = (self.cursor + 1) % count,
            "Home" | "PageUp" if plain => self.cursor = 0,
            "End" | "PageDown" if plain => self.cursor = count - 1,
            "Enter" | "Space" if plain => return MenuKey::Run(self.cursor),
            _ => {}
        }
        MenuKey::Stay
    }
}

/// The most entries the card shows, which the place keeps room for.
fn most_entries() -> usize {
    entries(true, Level::Main)
        .len()
        .max(entries(true, Level::Sizes).len())
}

/// Where the menu stands for the bar at `place`, laid out at `bar`, opened for the button at
/// `button` or beside the buttons: under the button (beside it in the vertical bar), else at the
/// bar's start; kept inside the bar's length.
pub fn spot(place: BarPlace, bar: Rectangle, button: Option<Rectangle>) -> Point {
    let card = most_entries() as f32 * theme::ROW_HEIGHT + 2.0 * MENU_PADDING + 2.0;
    match place {
        BarPlace::Horizontal => {
            let (x, y) = match button {
                Some(button) => (button.x, button.y + button.height + GAP),
                None => (bar.x + INDENT, bar.y + bar.height + GAP),
            };
            let x = x.min(bar.x + bar.width - MENU_WIDTH).max(bar.x);
            Point::new(x, y)
        }
        BarPlace::Vertical => {
            let y = button.map_or(bar.y + INDENT, |button| button.y);
            let y = y.min(bar.y + bar.height - card).max(bar.y);
            Point::new(bar.x + bar.width + GAP, y)
        }
    }
}

/// Measures the bar at `place` and the button at `button`, and answers where the menu stands.
fn measure(place: BarPlace, button: Option<usize>) -> Task<Message> {
    let bounds = Bounds {
        bar: view::bar_id(place),
        button: button.map(|at| view::item_id(place, at)),
        found_bar: None,
        found_button: None,
    };
    operate(bounds).map(move |(bar, item)| {
        let at = bar.map(|bar| spot(place, bar, item));
        wrap(BbMsg::Ctx(CtxMsg::Placed(at)))
    })
}

/// A widget operation that finds the bar and the button.
struct Bounds {
    bar: Id,
    button: Option<Id>,
    found_bar: Option<Rectangle>,
    found_button: Option<Rectangle>,
}

impl Operation<(Option<Rectangle>, Option<Rectangle>)> for Bounds {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn Operation<(Option<Rectangle>, Option<Rectangle>)>),
    ) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        let Some(id) = id else {
            return;
        };
        if *id == self.bar {
            self.found_bar = Some(bounds);
        } else if self.button.as_ref() == Some(id) {
            self.found_button = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<(Option<Rectangle>, Option<Rectangle>)> {
        Outcome::Some((self.found_bar, self.found_button))
    }
}

impl App {
    /// The context menu of the bar at `place`, for the button at `button` or beside the buttons;
    /// a separator counts as beside them. Any other menu of the bars closes.
    pub(super) fn open_bar_context(
        &mut self,
        place: BarPlace,
        button: Option<usize>,
    ) -> Vec<Effect> {
        let Some(bar) = self.button_bar.shown(place) else {
            return Vec::new();
        };
        let button = button.filter(|&at| {
            bar.spec
                .items
                .get(at)
                .is_some_and(|item| item.button().is_some())
        });
        let menu = BarMenu::new(place, bar.id, button);
        self.button_bar.menu = None;
        self.button_bar.context = Some(menu);
        vec![Effect::Task(measure(place, button))]
    }

    /// A message of the open context menu.
    pub(super) fn on_bar_context(&mut self, message: CtxMsg) -> Vec<Effect> {
        let ui = &mut self.button_bar;
        let Some(menu) = ui.context.as_mut() else {
            return Vec::new();
        };
        match message {
            CtxMsg::Placed(Some(at)) => menu.at = Some(at),
            // The bar is not shown, or a click beside the menu.
            CtxMsg::Placed(None) | CtxMsg::Close => ui.context = None,
            CtxMsg::Hover(at) => {
                if at < menu.items(&ui.settings).len() {
                    menu.cursor = at;
                }
            }
            CtxMsg::Pick(at) => return self.run_context_entry(at),
        }
        Vec::new()
    }

    /// A key while the context menu is open: every key is the menu's.
    pub(super) fn bar_context_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let ui = &mut self.button_bar;
        let Some(menu) = ui.context.as_mut() else {
            return Vec::new();
        };
        let items = menu.items(&ui.settings);
        match menu.key(chord, &items) {
            MenuKey::Stay => Vec::new(),
            MenuKey::Close => {
                ui.context = None;
                Vec::new()
            }
            MenuKey::Run(at) => self.run_context_entry(at),
        }
    }

    /// The entry `at` of the open menu: Symbol size opens its sizes, every other entry closes
    /// the menu and does what it says.
    fn run_context_entry(&mut self, at: usize) -> Vec<Effect> {
        let ui = &mut self.button_bar;
        let Some(menu) = ui.context.as_mut() else {
            return Vec::new();
        };
        let Some(item) = menu.items(&ui.settings).into_iter().nth(at) else {
            return Vec::new();
        };
        if item.entry == Entry::IconSize {
            menu.open_sizes(&ui.settings);
            return Vec::new();
        }
        let BarMenu {
            place, bar, button, ..
        } = *menu;
        ui.context = None;
        let set = |key: &str, value: Value| Command::SetSetting {
            key: key.to_owned(),
            value,
        };
        match (item.entry, button) {
            (Entry::Change, Some(button)) => self.open_button_bar_window(Some((bar, Some(button)))),
            (Entry::Delete, Some(button)) => self.delete_button(bar, button),
            (Entry::Insert, Some(button)) => self.insert_in_bar_window(bar, button),
            (Entry::EditBar, _) => self.open_button_bar_window(Some((bar, None))),
            (Entry::Flat, _) => {
                let flat = !self.button_bar.settings.flat;
                vec![Effect::Dispatch(
                    self.active,
                    vec![set(FLAT_KEY, Value::Bool(flat))],
                )]
            }
            (Entry::Size(size), _) => vec![Effect::Dispatch(
                self.active,
                vec![set(ICON_SIZE_KEY, Value::from(size))],
            )],
            (Entry::OtherSize, _) => {
                let mut effects = self.apply(Message::Settings(SettingsMessage::OpenPage(
                    Page::Appearance,
                )));
                effects.extend(self.apply(Message::Settings(SettingsMessage::Tab(Tab::Window))));
                effects
            }
            (Entry::Hide, _) => {
                let kind = match place {
                    BarPlace::Horizontal => BarKind::ButtonBar,
                    BarPlace::Vertical => BarKind::VerticalButtonBar,
                };
                if self
                    .button_bar
                    .focus
                    .is_some_and(|focus| focus.place == place)
                {
                    self.button_bar.focus = None;
                }
                // Off, whatever the window last read: the bar is shown, so its setting is on.
                vec![Effect::Dispatch(
                    self.active,
                    vec![set(kind.setting_key(), Value::Bool(false))],
                )]
            }
            // A button's entry without a button is never shown.
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
