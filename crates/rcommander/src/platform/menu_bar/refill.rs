//! The Workspaces menu filled again after the workspaces changed (RC-100-68): muda keeps the
//! entries of a menu once it is built, so a menu whose list is the person's own is emptied and
//! filled anew on the event loop's thread, where the bar lives (`BAR`).

use iced::{Task, window};
use muda::{MenuItem, PredefinedMenuItem, Submenu};

use super::{BAR, BarItem, BarMenu, entry_text};

/// Fills each menu of the attached bar of the main window `id` whose entries are not those of
/// `menus` - the Workspaces menu after the workspaces changed (RC-100-68) - with `menus`' entries
/// again; every other menu keeps its own. A menu muda cannot fill keeps what it had.
pub fn refill(id: window::Id, menus: Vec<BarMenu>) -> Task<()> {
    window::run(id, move |_window| {
        BAR.with_borrow(|bar| {
            let Some(bar) = bar else {
                return;
            };
            for (submenu, menu) in bar.submenus.iter().zip(&menus) {
                let shown: Vec<String> = submenu
                    .items()
                    .iter()
                    .filter_map(|item| item.as_menuitem().map(|item| item.id().0.clone()))
                    .collect();
                let wanted: Vec<&str> = menu
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        BarItem::Entry { id, .. } => Some(*id),
                        BarItem::Separator => None,
                    })
                    .collect();
                if shown.iter().map(String::as_str).eq(wanted.iter().copied()) {
                    continue;
                }
                while submenu.remove_at(0).is_some() {}
                for item in &menu.items {
                    if append(submenu, item).is_err() {
                        break;
                    }
                }
            }
        });
    })
}

/// `item` added at the end of `submenu`, without an accelerator: the key stays the keymap's.
fn append(submenu: &Submenu, item: &BarItem) -> muda::Result<()> {
    match item {
        BarItem::Entry { id, text } => {
            submenu.append(&MenuItem::with_id(*id, entry_text(text), true, None))
        }
        BarItem::Separator => submenu.append(&PredefinedMenuItem::separator()),
    }
}
