//! The tree's context menu (RC-100-63): a right click on a row, Shift+F10 or the context-menu
//! key opens it at the cursor's row. It lists the toolbar's actions and a new connection, a new
//! folder and a copy, each with its key; an action that cannot run is greyed out and passed by
//! the arrows. Drawn and worked like the file list's own menu (`app/context_menu/`): the arrows,
//! Home and End move, Enter or Space run the entry, Escape closes; any other key closes it and
//! works the tree. Opening measures the tree's side and its list, so the card stands under the
//! row even in a scrolled list ([`spot`]).

use iced::{
    Point, Rectangle, Task, Vector,
    advanced::widget::{
        Id, Operation, operate,
        operation::{Outcome, Scrollable},
    },
};
use rc_engine::Chord;

use super::{
    ConnMsg, ConnectionManager,
    tree_edit::{TreeAction, TreeMsg},
    view::{TREE_SCROLL_ID, TREE_SIDE_ID},
};
use crate::{
    app::{Effect, Message},
    theme,
};

/// The menu's width.
pub const MENU_WIDTH: f32 = 260.0;
/// The card's padding and border above and below its entries.
const CARD_FRAME: f32 = 2.0 * 4.0 + 2.0;
/// Pixels a level of the tree indents (`view/tree.rs`) and the room of a row's sign.
const INDENT: f32 = 16.0;
const SIGN: f32 = 24.0;
/// The space between two rows of the tree.
const ROW_SPACING: f32 = 1.0;

/// The open menu.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeMenu {
    /// The entry the keyboard or the pointer is on, an index of [`TreeAction::MENU`].
    pub cursor: usize,
    /// Its top-left corner in the tree's side; `None` until measured.
    pub at: Option<Point>,
}

impl ConnectionManager {
    /// Shift+F10, the context-menu key or a right click: the menu opens at the cursor's row,
    /// on its first entry that can run.
    pub(super) fn open_menu(&mut self) -> Vec<Effect> {
        self.pending = None;
        let cursor = (0..TreeAction::MENU.len())
            .find(|&at| self.can(TreeAction::MENU[at]))
            .unwrap_or(0);
        self.edit.menu = Some(TreeMenu { cursor, at: None });
        let rows = self.tree.rows();
        let (row, depth) = self
            .tree
            .cursor_index(&rows)
            .map_or((0, 0), |at| (at, rows[at].depth));
        vec![Effect::Task(measure(row, depth))]
    }

    /// The menu's own messages.
    pub(super) fn menu_message(&mut self, message: TreeMsg) -> Vec<Effect> {
        let Some(menu) = self.edit.menu.as_mut() else {
            return Vec::new();
        };
        match message {
            TreeMsg::Placed(Some(at)) => menu.at = Some(at),
            TreeMsg::Placed(None) | TreeMsg::MenuClose => self.edit.menu = None,
            TreeMsg::MenuHover(at) if at < TreeAction::MENU.len() => menu.cursor = at,
            _ => {}
        }
        Vec::new()
    }

    /// A key while the menu is open: `None` when it closed the menu and the tree takes the key.
    pub(super) fn menu_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let menu = self.edit.menu.as_ref()?;
        let plain = !chord.ctrl && !chord.alt && !chord.shift;
        let count = TreeAction::MENU.len();
        let runs = |at: usize| self.can(TreeAction::MENU[at]);
        let step = |from: usize, forward: bool| {
            let order: Vec<usize> = if forward {
                (from + 1..count).collect()
            } else {
                (0..from).rev().collect()
            };
            order.into_iter().find(|&at| runs(at)).unwrap_or(from)
        };
        let cursor = menu.cursor;
        let moved = match chord.key.as_str() {
            "ArrowDown" if plain => Some(step(cursor, true)),
            "ArrowUp" if plain => Some(step(cursor, false)),
            "Home" | "PageUp" if plain => Some((0..count).find(|&at| runs(at)).unwrap_or(cursor)),
            "End" | "PageDown" if plain => {
                Some((0..count).rev().find(|&at| runs(at)).unwrap_or(cursor))
            }
            _ => None,
        };
        if let Some(to) = moved {
            if let Some(menu) = self.edit.menu.as_mut() {
                menu.cursor = to;
            }
            return Some(Vec::new());
        }
        let action = TreeAction::MENU[cursor.min(count - 1)];
        let can = self.can(action);
        self.edit.menu = None;
        match chord.key.as_str() {
            "Enter" | "Space" if plain && can => Some(self.act(action)),
            "Enter" | "Space" | "Escape" | "ContextMenu" if plain => Some(Vec::new()),
            "F10" if chord.shift && !chord.ctrl && !chord.alt => Some(Vec::new()),
            _ => None,
        }
    }
}

/// Where a menu of `entries` opens for the row `row` at `depth` in a list scrolled by
/// `scrolled` pixels: just under the row, beside its sign, in the tree's side `side`, whose list
/// is `list` (both in window coordinates). Kept inside the side: moved up when the card would
/// pass its bottom; at the list's top when the row is scrolled out of sight.
pub fn spot(
    side: Rectangle,
    list: Rectangle,
    scrolled: f32,
    (row, depth): (usize, usize),
    entries: usize,
) -> Point {
    let card = entries as f32 * theme::ROW_HEIGHT + CARD_FRAME;
    let pitch = theme::ROW_HEIGHT + ROW_SPACING;
    let under = list.y + (row as f32 + 1.0) * pitch - scrolled;
    let under = if under < list.y || under > list.y + list.height {
        list.y
    } else {
        under
    };
    let x = (list.x + INDENT * depth as f32 + SIGN)
        .min(side.x + side.width - MENU_WIDTH)
        .max(side.x);
    let y = under.min(side.y + side.height - card).max(side.y);
    Point::new(x - side.x, y - side.y)
}

/// Measures the tree's side and list, and answers where the menu for the row `row` at `depth`
/// stands.
fn measure(row: usize, depth: usize) -> Task<Message> {
    operate(Bounds::default()).map(move |(side, list)| {
        let at = side.zip(list).map(|(side, (list, scrolled))| {
            spot(side, list, scrolled.y, (row, depth), TreeAction::MENU.len())
        });
        Message::Connections(ConnMsg::Tree(TreeMsg::Placed(at)))
    })
}

/// What [`Bounds`] finds: the side's bounds, the list's bounds and how far it is scrolled.
type Found = (Option<Rectangle>, Option<(Rectangle, Vector)>);

/// A widget operation that finds the tree's side and its scrolled list.
#[derive(Default)]
struct Bounds {
    side: Option<Rectangle>,
    list: Option<(Rectangle, Vector)>,
}

impl Operation<Found> for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Found>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::new(TREE_SIDE_ID)) {
            self.side = Some(bounds);
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
        if id == Some(&Id::new(TREE_SCROLL_ID)) {
            self.list = Some((bounds, translation));
        }
    }

    fn finish(&self) -> Outcome<Found> {
        Outcome::Some((self.side, self.list))
    }
}

#[cfg(test)]
#[path = "tree_menu_tests.rs"]
mod tests;
