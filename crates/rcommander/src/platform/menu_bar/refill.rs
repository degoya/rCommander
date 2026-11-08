//! The Workspaces menu (RC-100-68) and the Start menu (RC-100-86) filled again after their lists
//! changed: muda keeps the entries of a menu once it is built, so a menu whose list is the
//! person's own is emptied and filled anew on the event loop's thread, where the bar lives
//! (`BAR`). Also how an entry is added - a submenu with its entries - and relabelled, for the bar's
//! first build and a changed keymap.

use iced::{Task, window};
use muda::{MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use rc_engine::START_KEY_PREFIX;

use super::{BAR, BarItem, BarMenu, entry_text};

/// Fills each menu of the attached bar of the main window `id` whose entries are not those of
/// `menus` - the Workspaces menu after the workspaces changed - with `menus`' entries again, and
/// the Start menu every time: an entry's title or a submenu changes under the same ids. Every
/// other menu keeps its own. A menu muda cannot fill keeps what it had.
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
                        BarItem::Separator | BarItem::Submenu { .. } => None,
                    })
                    .collect();
                let same = shown.iter().map(String::as_str).eq(wanted.iter().copied());
                if same && !always(menu) {
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

/// Whether `menu` is filled again whatever its ids: it holds a submenu or a Start menu entry.
fn always(menu: &BarMenu) -> bool {
    menu.items.iter().any(|item| match item {
        BarItem::Submenu { .. } => true,
        BarItem::Entry { id, .. } => id.starts_with(START_KEY_PREFIX),
        BarItem::Separator => false,
    })
}

/// `item` added at the end of `submenu`, without an accelerator: the key stays the keymap's. A
/// submenu is added with its entries.
pub(super) fn append(submenu: &Submenu, item: &BarItem) -> muda::Result<()> {
    match item {
        BarItem::Entry { id, text } => {
            submenu.append(&MenuItem::with_id(*id, entry_text(text), true, None))
        }
        BarItem::Separator => submenu.append(&PredefinedMenuItem::separator()),
        BarItem::Submenu { text, items } => {
            let nested = Submenu::new(text, true);
            for item in items {
                append(&nested, item)?;
            }
            submenu.append(&nested)
        }
    }
}

/// Gives each entry of `items`, in their submenus too, the text `menus` has for its id: the keys
/// of a changed keymap (RC-100-21). An entry `menus` does not name keeps its text.
pub(super) fn relabel_items(items: &[MenuItemKind], menus: &[BarMenu]) {
    for item in items {
        if let Some(nested) = item.as_submenu() {
            relabel_items(&nested.items(), menus);
            continue;
        }
        let Some(item) = item.as_menuitem() else {
            continue;
        };
        let text = menus
            .iter()
            .find_map(|menu| text_of(&menu.items, &item.id().0));
        if let Some(text) = text {
            item.set_text(entry_text(text));
        }
    }
}

/// The text of the entry `id` among `items`, in their submenus too.
fn text_of<'m>(items: &'m [BarItem], id: &str) -> Option<&'m str> {
    items.iter().find_map(|item| match item {
        BarItem::Entry { id: at, text } if *at == id => Some(text.as_str()),
        BarItem::Submenu { items, .. } => text_of(items, id),
        _ => None,
    })
}

#[cfg(test)]
#[path = "refill_tests.rs"]
mod tests;
