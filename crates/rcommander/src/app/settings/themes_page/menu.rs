//! The context menu of a theme's row (RC-110-09): a right click on the row, or Shift+F10 and the
//! context-menu key while the list has the focus ring or no control has it, opens it for that
//! theme - "Use for ..." for every environment the tab's pick lists assign it to (`usage.rs`), a
//! check mark where it is used already, then the row's buttons Edit..., Duplicate, Rename and
//! Delete under their rules. A "Use for ..." edits the draft as that environment's pick list does;
//! one checked changes nothing.
//!
//! It is the file list's own menu in a dialog (`app/context_menu/`, as the connection tree's,
//! `connections/tree_menu.rs`): the same width and card, the arrows, Home and End move past the
//! entries greyed out, a letter takes or moves (`menu_letter`), Enter or Space take the entry,
//! Escape, Shift+F10 and the menu key close it; every other key is swallowed. Opening measures
//! the row and the tab's scrolled area, so the card stands under the row - over it where it
//! would pass the area's bottom - even in a scrolled page ([`spot`]). Drawn by `menu_view.rs`.

use iced::{
    Point, Rectangle, Task, Vector,
    advanced::widget::{
        Id, Operation, operate,
        operation::{Outcome, Scrollable},
    },
};
use rc_engine::{
    Chord,
    settings::themes::{Theme, ThemesState},
};

use super::{
    ThemesMsg, ThemesPage, page_message,
    usage::{self, Environment},
    wsl,
};
use crate::{
    app::{
        App, Effect, Message,
        context_menu::{MENU_PADDING, MENU_WIDTH},
        field_walk,
        menu_letter::{self, Letter},
        settings::{Page, SettingsMessage, Tab, draft::Draft},
    },
    i18n::t,
    theme,
};

/// The id of the theme list's stop of the focus ring.
pub const LIST_STOP: &str = "settings.themes.list";
/// The id of the Themes tab's scrolled area, over which the menu stands.
pub const SCROLL_ID: &str = "settings.themes.scroll";
/// How far right of the row's left edge the menu starts.
const INDENT: f32 = 24.0;
/// The card's padding and border above and below its entries.
const CARD_FRAME: f32 = 2.0 * MENU_PADDING + 2.0;

/// The id of the row of the theme `id`, measured to place the menu.
pub fn row_id(id: &str) -> Id {
    Id::from(format!("settings.themes.row.{id}"))
}

/// What an entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// The theme for this environment.
    Use(Environment),
    Edit,
    Duplicate,
    Rename,
    Delete,
}

/// One entry: what it does, its label, whether the theme is used there already, whether it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub entry: Entry,
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
}

/// The open menu.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeMenu {
    /// The id of the theme it is for.
    pub theme: String,
    pub items: Vec<Item>,
    /// The entry the keyboard or the pointer is on.
    pub cursor: usize,
    /// Its top-left corner in the tab's scrolled area; `None` until measured.
    pub at: Option<Point>,
}

#[derive(Debug, Clone)]
pub enum MenuMsg {
    /// A right click on the row of the theme with this id.
    Open(String),
    /// Shift+F10 or the context-menu key: the id of the stop that has the keyboard.
    Wanted(Option<Id>),
    /// Where the menu stands; `None` when the row is not shown.
    Placed(Option<Point>),
    /// The pointer is over this entry.
    Hover(usize),
    /// A click on this entry.
    Pick(usize),
    /// A click beside the menu, a scroll.
    Close,
}

/// What a key does to the open menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    /// It stays open; the cursor may have moved.
    Stay,
    Close,
    /// The menu closes and this entry runs.
    Run(usize),
}

/// The entries for `theme`: "Use for ..." each environment it can be used for, checked where the
/// draft uses it, then the row's buttons, enabled as the buttons are.
pub(super) fn items(
    page: &ThemesPage,
    theme: &Theme,
    draft: &Draft,
    state: &ThemesState,
) -> Vec<Item> {
    let distros: &[String] = if wsl::SHOWN { &page.distros } else { &[] };
    let uses = usage::offered(theme, distros)
        .into_iter()
        .map(|environment| Item {
            label: environment.use_label(),
            checked: usage::uses(&environment, &theme.id, draft, state),
            enabled: true,
            entry: Entry::Use(environment),
        });
    let ready = !page.busy;
    let own = !theme.builtin;
    let in_use = page.chosen(&theme.id);
    let actions = [
        (Entry::Edit, t!("settings.themes.edit"), own),
        (Entry::Duplicate, t!("settings.themes.duplicate"), true),
        (Entry::Rename, t!("settings.themes.rename"), own),
        (Entry::Delete, t!("settings.themes.delete"), own && !in_use),
    ]
    .into_iter()
    .map(|(entry, label, can)| Item {
        entry,
        label: label.to_owned(),
        checked: false,
        enabled: ready && can,
    });
    uses.chain(actions).collect()
}

/// Shift+F10 or the context-menu key.
pub fn is_menu_key(chord: &Chord) -> bool {
    match chord.key.as_str() {
        "F10" => chord.shift && !chord.ctrl && !chord.alt,
        "ContextMenu" => chord.is_plain(),
        _ => false,
    }
}

impl ThemeMenu {
    /// The menu for the theme `theme` with `items`, on its first entry that runs.
    pub fn new(theme: String, items: Vec<Item>) -> Self {
        let cursor = items.iter().position(|item| item.enabled).unwrap_or(0);
        Self {
            theme,
            items,
            cursor,
            at: None,
        }
    }

    /// The entry that runs after (`forward`) or before the cursor, round the end; the cursor's own
    /// when none other runs.
    fn step(&self, forward: bool) -> usize {
        let count = self.items.len();
        (1..count)
            .map(|by| {
                if forward {
                    (self.cursor + by) % count
                } else {
                    (self.cursor + count - by) % count
                }
            })
            .find(|&at| self.items[at].enabled)
            .unwrap_or(self.cursor)
    }

    /// The first (`last`: the last) entry that runs.
    fn end(&self, last: bool) -> usize {
        let runs = |at: &usize| self.items[*at].enabled;
        let found = if last {
            (0..self.items.len()).rev().find(runs)
        } else {
            (0..self.items.len()).find(runs)
        };
        found.unwrap_or(self.cursor)
    }

    /// A key while the menu is open, as the file list's menu takes it; an entry greyed out is
    /// passed by the arrows and the letters.
    pub fn key(&mut self, chord: &Chord) -> MenuKey {
        if menu_letter::is_letter(chord) {
            let labels = self
                .items
                .iter()
                .map(|item| item.enabled.then_some(item.label.as_str()));
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
        let moved = match chord.key.as_str() {
            "Escape" => return MenuKey::Close,
            "ArrowUp" if plain => self.step(false),
            "ArrowDown" if plain => self.step(true),
            "Home" | "PageUp" if plain => self.end(false),
            "End" | "PageDown" if plain => self.end(true),
            "Enter" | "Space" if plain => {
                let runs = self.items.get(self.cursor).is_some_and(|item| item.enabled);
                return if runs {
                    MenuKey::Run(self.cursor)
                } else {
                    MenuKey::Close
                };
            }
            _ => return MenuKey::Stay,
        };
        self.cursor = moved;
        MenuKey::Stay
    }
}

/// Where a menu of `entries` stands for the row `row` in the tab's scrolled area `area`, scrolled
/// by `scrolled` (both as laid out, in window coordinates): under the row, indented; over it where
/// it would pass the area's bottom; at the area's top when the row is scrolled out of sight. Kept
/// inside the area, relative to its top-left corner.
pub fn spot(area: Rectangle, scrolled: Vector, row: Rectangle, entries: usize) -> Point {
    let card = entries as f32 * theme::ROW_HEIGHT + CARD_FRAME;
    let top = row.y - scrolled.y;
    let bottom = top + row.height;
    let floor = area.y + area.height;
    let y = if bottom < area.y || top > floor {
        area.y
    } else if bottom + card <= floor {
        bottom
    } else {
        top - card
    };
    let y = y.min(floor - card).max(area.y);
    let x = (row.x - scrolled.x + INDENT)
        .min(area.x + area.width - MENU_WIDTH)
        .max(area.x);
    Point::new(x - area.x, y - area.y)
}

/// Measures the row of the theme `id` and the tab's scrolled area, and answers where the menu of
/// `entries` stands.
fn measure(id: &str, entries: usize) -> Task<Message> {
    let bounds = Bounds {
        row: row_id(id),
        area: None,
        found: None,
    };
    operate(bounds).map(move |(area, row)| {
        let at = area
            .zip(row)
            .map(|((area, scrolled), row)| spot(area, scrolled, row, entries));
        page_message(ThemesMsg::Menu(MenuMsg::Placed(at)))
    })
}

/// What [`Bounds`] finds: the scrolled area with how far it is scrolled, and the row.
type Found = (Option<(Rectangle, Vector)>, Option<Rectangle>);

/// A widget operation that finds the tab's scrolled area and the row with the id `row`.
struct Bounds {
    row: Id,
    area: Option<(Rectangle, Vector)>,
    found: Option<Rectangle>,
}

impl Operation<Found> for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Found>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.found = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&Id::new(SCROLL_ID)) {
            self.area = Some((bounds, translation));
        }
    }

    fn finish(&self) -> Outcome<Found> {
        Outcome::Some((self.area, self.found))
    }
}

impl App {
    /// Whether the settings dialog shows the Themes tab.
    fn themes_shown(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|dialog| dialog.page == Page::Appearance)
            && self.settings_tabs.chosen(Page::Appearance) == Some(Tab::Themes)
    }

    /// The Themes tab's keys: while the menu is open every key is the menu's; else Shift+F10 and
    /// the context-menu key ask which stop has the keyboard. `None` for a key the dialog takes.
    pub(in crate::app::settings) fn themes_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        if !self.themes_shown() {
            return None;
        }
        let page = &mut self.settings.as_mut()?.themes;
        if page.editor.is_some() {
            return None;
        }
        let action = page.menu.as_mut().map(|menu| menu.key(chord));
        match action {
            Some(MenuKey::Stay) => Some(Vec::new()),
            Some(MenuKey::Close) => {
                page.menu = None;
                Some(Vec::new())
            }
            Some(MenuKey::Run(at)) => Some(self.run_theme_entry(at)),
            None if is_menu_key(chord) => {
                let wanted = field_walk::focused()
                    .map(|id| page_message(ThemesMsg::Menu(MenuMsg::Wanted(id))));
                Some(vec![Effect::Task(wanted)])
            }
            None => None,
        }
    }

    /// A message of the row's menu.
    pub(super) fn theme_menu_message(&mut self, message: MenuMsg) -> Vec<Effect> {
        match message {
            MenuMsg::Open(id) => return self.open_theme_menu(Some(id)),
            // The list has the ring, or nothing has it (after a click).
            MenuMsg::Wanted(focused) => {
                if focused.is_none_or(|id| id == Id::new(LIST_STOP)) {
                    return self.open_theme_menu(None);
                }
            }
            MenuMsg::Pick(at) => return self.run_theme_entry(at),
            other @ (MenuMsg::Placed(_) | MenuMsg::Hover(_) | MenuMsg::Close) => {
                let Some(page) = self.settings.as_mut().map(|dialog| &mut dialog.themes) else {
                    return Vec::new();
                };
                let open = match (other, page.menu.as_mut()) {
                    (MenuMsg::Placed(Some(at)), Some(menu)) => {
                        menu.at = Some(at);
                        true
                    }
                    (MenuMsg::Hover(at), Some(menu)) => {
                        if menu.items.get(at).is_some_and(|item| item.enabled) {
                            menu.cursor = at;
                        }
                        true
                    }
                    // Not measured (the row is not shown), or a click beside it.
                    _ => false,
                };
                if !open {
                    page.menu = None;
                }
            }
        }
        Vec::new()
    }

    /// Opens the menu for the theme `id` - marked first, as a click marks it - or for the marked
    /// one; the row is measured before it shows.
    fn open_theme_menu(&mut self, id: Option<String>) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.themes;
        if page.editor.is_some() {
            return Vec::new();
        }
        if let Some(id) = id {
            page.selected = Some(id);
            page.renaming = None;
        }
        let (Some(draft), Some(state), Some(theme)) =
            (dialog.draft.as_ref(), page.state.as_ref(), page.marked())
        else {
            return Vec::new();
        };
        let items = items(page, theme, draft, state);
        let id = theme.id.clone();
        let count = items.len();
        page.menu = Some(ThemeMenu::new(id.clone(), items));
        vec![Effect::Task(measure(&id, count))]
    }

    /// The entry `at` of the open menu, which closes: the theme used for an environment as its
    /// pick list would set it, or the row's button pressed for the menu's theme.
    fn run_theme_entry(&mut self, at: usize) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let Some(menu) = dialog.themes.menu.take() else {
            return Vec::new();
        };
        let theme = menu.theme;
        let Some(item) = menu.items.into_iter().nth(at).filter(|item| item.enabled) else {
            return Vec::new();
        };
        dialog.themes.selected = Some(theme.clone());
        let pressed = match item.entry {
            Entry::Use(_) if item.checked => return Vec::new(),
            Entry::Use(environment) => {
                let (Some(draft), Some(state)) =
                    (dialog.draft.as_ref(), dialog.themes.state.as_ref())
                else {
                    return Vec::new();
                };
                let (key, text) = usage::assign(&environment, &theme, draft, state);
                return self.settings_message(SettingsMessage::Edit(key, text));
            }
            Entry::Edit => ThemesMsg::Edit,
            Entry::Duplicate => ThemesMsg::Duplicate,
            Entry::Rename => ThemesMsg::Rename,
            Entry::Delete => ThemesMsg::Delete,
        };
        self.themes_action(pressed)
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
