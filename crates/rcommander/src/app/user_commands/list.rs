//! The window's lists (RC-100-86): the own commands narrowed by the filter, and the Start menu as
//! a tree - its rows in menu order, each with its depth -, the cursor kept on a row by its id, and
//! where a row goes when it moves up, down, into the submenu above it or out of its own; all of it
//! as `start_menu.move` takes it: a parent and a place among the parent's other rows.

use rc_engine::{StartItem, StartItemId, StartItemKind, UserCommand};

/// Rows a page key moves.
pub const PAGE: isize = 10;

/// The ids of `commands` whose name, tooltip or command holds every word of `filter`, ignoring
/// case, in their order.
pub fn shown_commands(commands: &[UserCommand], filter: &str) -> Vec<i64> {
    let filter = filter.to_lowercase();
    let words: Vec<&str> = filter.split_whitespace().collect();
    commands
        .iter()
        .filter(|command| {
            let text = format!(
                "{} {} {}",
                command.name, command.tooltip, command.action.command
            )
            .to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .map(|command| command.id)
        .collect()
}

/// The ids of the Start menu's rows, in menu order.
pub fn shown_items(items: &[StartItem]) -> Vec<i64> {
    items.iter().map(|item| item.id).collect()
}

/// The cursor among `shown`: `cursor` while it is shown, else the first; `None` for an empty
/// list.
pub fn keep(shown: &[i64], cursor: Option<i64>) -> Option<i64> {
    cursor
        .filter(|id| shown.contains(id))
        .or_else(|| shown.first().copied())
}

/// The cursor moved by `by` rows among `shown`, staying in the list; the first row when the
/// cursor is on none of them.
pub fn step(shown: &[i64], cursor: Option<i64>, by: isize) -> Option<i64> {
    let Some(at) = cursor.and_then(|id| shown.iter().position(|shown| *shown == id)) else {
        return shown.first().copied();
    };
    let last = shown.len().saturating_sub(1);
    shown.get(at.saturating_add_signed(by).min(last)).copied()
}

/// A place in the Start menu as `start_menu.move` takes it: the submenu (`None`: the top) and the
/// place among its other rows, from 0 (`None`: after the last).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub parent: Option<StartItemId>,
    pub position: Option<usize>,
}

/// Where a row of the Start menu moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    /// Into the submenu right above it, as its last row.
    Into,
    /// Out of its submenu, right after it.
    Out,
}

/// The rows of the submenu `parent` (`None`: the top), in their order.
pub fn siblings(items: &[StartItem], parent: Option<StartItemId>) -> Vec<StartItemId> {
    items
        .iter()
        .filter(|item| item.parent == parent)
        .map(|item| item.id)
        .collect()
}

/// The submenu `id` is in and its place among that submenu's rows; `None` for a row not listed.
fn place_of(items: &[StartItem], id: StartItemId) -> Option<(Option<StartItemId>, usize)> {
    let parent = items.iter().find(|item| item.id == id)?.parent;
    let at = siblings(items, parent).iter().position(|row| *row == id)?;
    Some((parent, at))
}

/// Where `id` goes for `to`; `None` where it cannot go: up at the first place, down at the last,
/// into a submenu with none right above it, out of the top.
pub fn moved(items: &[StartItem], id: StartItemId, to: Move) -> Option<Place> {
    let (parent, at) = place_of(items, id)?;
    let rows = siblings(items, parent);
    match to {
        Move::Up => at.checked_sub(1).map(|position| Place {
            parent,
            position: Some(position),
        }),
        Move::Down => (at + 1 < rows.len()).then_some(Place {
            parent,
            position: Some(at + 1),
        }),
        Move::Into => {
            let above = at.checked_sub(1).and_then(|before| rows.get(before))?;
            let submenu = items
                .iter()
                .find(|item| item.id == *above)
                .filter(|item| item.kind == StartItemKind::Submenu)?;
            Some(Place {
                parent: Some(submenu.id),
                position: None,
            })
        }
        Move::Out => {
            let (above, place) = place_of(items, parent?)?;
            Some(Place {
                parent: above,
                position: Some(place + 1),
            })
        }
    }
}

/// Where a new row goes: right after the row at `cursor`, in its submenu; at the end of the top
/// without a cursor.
pub fn new_place(items: &[StartItem], cursor: Option<StartItemId>) -> Place {
    match cursor.and_then(|id| place_of(items, id)) {
        Some((parent, at)) => Place {
            parent,
            position: Some(at + 1),
        },
        None => Place {
            parent: None,
            position: None,
        },
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
