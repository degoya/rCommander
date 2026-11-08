//! The menus of a bar (RC-100-87): a sub-bar opens as a menu under its button with its symbols
//! and names, `»` the items the bar has no room for, and a right click or Shift+F10 the button's
//! own menu - Change, Delete, Insert button, Edit bar. A sub-bar's sub-bar opens over it; Escape
//! and Left go back one, a click beside the menu closes it. Deviation from Total Commander,
//! reasoned in the job file: a sub-bar always opens as a menu, TC's switch of the whole bar to it
//! (and back) is not made.

use iced::{Rectangle, advanced::widget::Id};
use rc_engine::{BarItem, BarPlace, BarSpec, ButtonAction, ButtonBarId, Chord, Command};

use super::{App, BarFocus, Effect, drop, keys, view};

/// What a menu lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// The items of bar `bar` from `from` on: a sub-bar from 0, `»` from the first hidden one.
    Items { bar: ButtonBarId, from: usize },
    /// The menu of the button at `button` of bar `bar`.
    Button { bar: ButtonBarId, button: usize },
}

impl MenuKind {
    /// The bar the menu is of.
    pub fn bar(self) -> ButtonBarId {
        match self {
            Self::Items { bar, .. } | Self::Button { bar, .. } => bar,
        }
    }
}

/// The entries of a button's menu, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEntry {
    Change,
    Delete,
    Insert,
    EditBar,
}

/// The button menu's entries.
pub const BUTTON_ENTRIES: [ButtonEntry; 4] = [
    ButtonEntry::Change,
    ButtonEntry::Delete,
    ButtonEntry::Insert,
    ButtonEntry::EditBar,
];

/// An open menu.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenMenu {
    pub kind: MenuKind,
    /// The bar it was opened from.
    pub place: BarPlace,
    /// The row at the cursor: an item's place in the bar, or an entry of the button's menu.
    pub cursor: usize,
    /// The menus it opened over, the first one first; Escape goes back to the last.
    pub back: Vec<MenuKind>,
    /// Under the button it opened from; `None` while that button is measured.
    pub under: Option<Rectangle>,
}

impl OpenMenu {
    fn new(kind: MenuKind, place: BarPlace, cursor: usize) -> Self {
        Self {
            kind,
            place,
            cursor,
            back: Vec::new(),
            under: None,
        }
    }

    /// The button it opens under was measured: the menu shows there, or under the bar's start.
    pub fn anchor(&mut self, bounds: Option<Rectangle>) {
        self.under = Some(bounds.unwrap_or(Rectangle::new(
            iced::Point::new(8.0, 40.0),
            iced::Size::new(1.0, 1.0),
        )));
    }
}

/// What the menu's rows and keys say.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuMsg {
    /// A row clicked or Enter on it.
    Pick(usize),
    /// A key while the menu is open.
    Key(Chord),
    /// A click beside the menu.
    Close,
}

/// The rows of a bar's items from `from` on: their places in the bar.
pub fn item_rows(spec: &BarSpec, from: usize) -> Vec<usize> {
    (from..spec.items.len()).collect()
}

/// The row after (`forward`) or before `cursor` among `rows` that is a button; `cursor` itself
/// at the end.
fn step(spec: &BarSpec, rows: &[usize], cursor: usize, forward: bool) -> usize {
    let is_button = |at: &&usize| spec.items.get(**at).and_then(BarItem::button).is_some();
    let found = if forward {
        rows.iter().filter(|at| **at > cursor).find(is_button)
    } else {
        rows.iter().rev().filter(|at| **at < cursor).find(is_button)
    };
    found.copied().unwrap_or(cursor)
}

impl App {
    /// The sub-bar `sub` of the button at `index` of the bar at `place`, as a menu under it.
    pub(super) fn open_sub_bar(
        &mut self,
        place: BarPlace,
        index: usize,
        sub: ButtonBarId,
    ) -> Vec<Effect> {
        let Some(bar) = self.button_bar.bar(sub) else {
            return Vec::new();
        };
        let cursor = keys::first(&bar.spec.items).unwrap_or(0);
        let menu = OpenMenu::new(MenuKind::Items { bar: sub, from: 0 }, place, cursor);
        self.show_menu(menu, view::item_id(place, index))
    }

    /// `»`: the bar's items from `from` on.
    pub(super) fn open_more(&mut self, place: BarPlace, from: usize) -> Vec<Effect> {
        let Some(bar) = self.button_bar.shown(place) else {
            return Vec::new();
        };
        let kind = MenuKind::Items { bar: bar.id, from };
        let cursor = (from..bar.spec.items.len())
            .find(|at| bar.spec.items[*at].button().is_some())
            .unwrap_or(from);
        self.show_menu(OpenMenu::new(kind, place, cursor), view::more_id(place))
    }

    /// The menu of the button at `index` of the bar at `place`.
    pub(super) fn open_button_menu(&mut self, place: BarPlace, index: usize) -> Vec<Effect> {
        let Some(bar) = self.button_bar.shown(place) else {
            return Vec::new();
        };
        let kind = MenuKind::Button {
            bar: bar.id,
            button: index,
        };
        self.show_menu(OpenMenu::new(kind, place, 0), view::item_id(place, index))
    }

    /// `menu` open under the container `under`, once that is measured.
    fn show_menu(&mut self, menu: OpenMenu, under: Id) -> Vec<Effect> {
        self.button_bar.menu = Some(menu);
        vec![Effect::Task(drop::measure(under))]
    }

    /// A row, a key or a click beside the menu.
    pub(super) fn on_bar_menu(&mut self, message: MenuMsg) -> Vec<Effect> {
        let Some(menu) = self.button_bar.menu.clone() else {
            return Vec::new();
        };
        match message {
            MenuMsg::Close => {
                self.button_bar.menu = None;
                Vec::new()
            }
            MenuMsg::Pick(row) => self.pick_bar_row(menu, row),
            MenuMsg::Key(chord) => self.bar_menu_key(menu, &chord),
        }
    }

    /// The keys of the open menu: Up and Down, Home and End, Enter or Right picks (a sub-bar
    /// opens over it), Left and Escape go back one, Escape on the first closes it.
    fn bar_menu_key(&mut self, mut menu: OpenMenu, chord: &Chord) -> Vec<Effect> {
        let plain = !chord.ctrl && !chord.alt && !chord.shift;
        let rows = self.menu_rows(&menu);
        let spec = self
            .button_bar
            .bar(menu.kind.bar())
            .map(|bar| bar.spec.clone());
        match chord.key.as_str() {
            "Escape" | "ArrowLeft" if plain => {
                match menu.back.pop() {
                    Some(kind) => {
                        menu.kind = kind;
                        menu.cursor = self.menu_rows(&menu).first().copied().unwrap_or(0);
                        self.button_bar.menu = Some(menu);
                    }
                    None => self.button_bar.menu = None,
                }
                return Vec::new();
            }
            "Enter" | "ArrowRight" | "Space" if plain => {
                return self.pick_bar_row(menu.clone(), menu.cursor);
            }
            "ArrowDown" | "ArrowUp" if plain => {
                let forward = chord.key == "ArrowDown";
                menu.cursor = match (&menu.kind, &spec) {
                    (MenuKind::Items { .. }, Some(spec)) => step(spec, &rows, menu.cursor, forward),
                    _ if forward => (menu.cursor + 1).min(rows.len().saturating_sub(1)),
                    _ => menu.cursor.saturating_sub(1),
                };
            }
            "Home" if plain => menu.cursor = rows.first().copied().unwrap_or(0),
            "End" if plain => menu.cursor = rows.last().copied().unwrap_or(0),
            _ => {}
        }
        self.button_bar.menu = Some(menu);
        Vec::new()
    }

    /// The rows of `menu`: the items' places, or the button menu's entries.
    fn menu_rows(&self, menu: &OpenMenu) -> Vec<usize> {
        match menu.kind {
            MenuKind::Items { bar, from } => self
                .button_bar
                .bar(bar)
                .map(|bar| item_rows(&bar.spec, from))
                .unwrap_or_default(),
            MenuKind::Button { .. } => (0..BUTTON_ENTRIES.len()).collect(),
        }
    }

    /// The row `row` of `menu` picked.
    fn pick_bar_row(&mut self, mut menu: OpenMenu, row: usize) -> Vec<Effect> {
        match menu.kind {
            MenuKind::Items { bar, .. } => {
                let Some(found) = self.button_bar.bar(bar) else {
                    self.button_bar.menu = None;
                    return Vec::new();
                };
                let Some(button) = found.spec.items.get(row).and_then(BarItem::button) else {
                    return Vec::new();
                };
                match &button.action {
                    ButtonAction::Command(_) => {
                        self.button_bar.menu = None;
                        self.leave_bar();
                        vec![self.run_button(bar, row, Vec::new())]
                    }
                    ButtonAction::SubBar(sub) => {
                        let sub = *sub;
                        let Some(inner) = self.button_bar.bar(sub) else {
                            return Vec::new();
                        };
                        menu.back.push(menu.kind);
                        menu.cursor = keys::first(&inner.spec.items).unwrap_or(0);
                        menu.kind = MenuKind::Items { bar: sub, from: 0 };
                        self.button_bar.menu = Some(menu);
                        Vec::new()
                    }
                }
            }
            MenuKind::Button { bar, button } => {
                self.button_bar.menu = None;
                match BUTTON_ENTRIES.get(row) {
                    Some(ButtonEntry::Change) => {
                        self.open_button_bar_window(Some((bar, Some(button))))
                    }
                    Some(ButtonEntry::Insert) => self.insert_in_bar_window(bar, button),
                    Some(ButtonEntry::EditBar) => self.open_button_bar_window(Some((bar, None))),
                    Some(ButtonEntry::Delete) => self.delete_button(bar, button),
                    None => Vec::new(),
                }
            }
        }
    }

    /// The keyboard leaves the bar it was in.
    fn leave_bar(&mut self) {
        self.button_bar.focus = None;
    }

    /// The button at `button` of bar `bar` removed: the bar saved without it.
    fn delete_button(&mut self, bar: ButtonBarId, button: usize) -> Vec<Effect> {
        let Some(found) = self.button_bar.bar(bar) else {
            return Vec::new();
        };
        if button >= found.spec.items.len() {
            return Vec::new();
        }
        let mut spec = found.spec.clone();
        spec.items.remove(button);
        let name = found.name.clone();
        if let Some(BarFocus { index, .. }) = self.button_bar.focus.as_mut()
            && *index >= spec.items.len()
        {
            *index = spec.items.len().saturating_sub(1);
        }
        vec![Effect::Dispatch(
            self.active,
            vec![Command::SaveButtonBar {
                id: Some(bar),
                name,
                spec,
            }],
        )]
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
