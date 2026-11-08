//! The own commands and the Start menu in the command palette and the menu bar (RC-100-86): one
//! row per own command, "Command: <tooltip or name>", running `usercmd.run { name }`, and one per
//! entry of the Start menu, "Start: <title>", running `start_menu.run { id }` - made from the
//! lists the window knows (`app/user_commands/known.rs`) each time the palette opens, as the
//! workspaces' rows are (`workspaces.rs`). A command or an entry with a key of its own (the
//! Shortcuts tab's "Add key for an own command..." and "...for a Start menu entry...") is that
//! key's row, which shows the key. `reach.json` names both commands `listed`.
//!
//! An own command's key is named by the command itself, `em_<name>`, a Start menu entry's
//! `start:<id>`, as the engine's keymap reads them (`rc_engine::own_command_key`, `start_key`);
//! the same words name them wherever the name of a key's row is shown ([`label`]). A key whose
//! command or entry was deleted names what it ran and is [`missing`].
//!
//! The menu bar's Start menu lists the entries after its own "Change Start menu..." and a
//! separator ([`menu_items`]), with their submenus and separators and without symbols; a title's
//! `&` marks its access key, as in Total Commander's Start menu.

use rc_engine::{
    Command, Keymap, StartItem, StartItemKind, UserCommand, own_command_key, start_key,
    start_key_name,
};

use super::{Action, Entry, workspaces::interned};
use crate::{app::user_commands::known, i18n::t, platform::menu_bar::BarItem};

/// The commands whose palette rows a list makes.
#[cfg(test)]
pub const LISTED: &[&str] = &["usercmd.run", "start_menu.run"];

/// The menu of the bar whose entries end with the Start menu's.
pub const MENU_ID: &str = "start";

/// What an own command is called: its tooltip, else its name.
pub fn shown_name(command: &UserCommand) -> &str {
    let tooltip = command.tooltip.trim();
    if tooltip.is_empty() {
        &command.name
    } else {
        tooltip
    }
}

/// A menu title as plain words: a single `&` (the access key's mark) dropped, `&&` an ampersand.
pub fn plain_title(title: &str) -> String {
    let mut plain = String::with_capacity(title.len());
    let mut chars = title.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            if let Some(next) = chars.next() {
                plain.push(next);
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

/// "Command: Notepad".
pub fn command_label(name: &str) -> String {
    t!("palette.user_command", name = name)
}

/// "Start: Notepad".
pub fn start_label(title: &str) -> String {
    t!("palette.start_item", title = plain_title(title))
}

/// The words of an own command's or a Start menu entry's key `name` with the lists the window
/// knows; `None` for the name of any other row.
pub fn label(name: &str) -> Option<String> {
    if own_command_key(name).is_none() && start_key(name).is_none() {
        return None;
    }
    label_in(name, &known::commands(), &known::start_menu())
}

/// The words of the key `name` among `commands` and `items`: the command's tooltip or name, the
/// entry's title; a deleted one by its name or id. `None` for the name of any other row.
pub fn label_in(name: &str, commands: &[UserCommand], items: &[StartItem]) -> Option<String> {
    if let Some(own) = own_command_key(name) {
        let shown = commands
            .iter()
            .find(|command| command.name == own)
            .map_or(own, shown_name);
        return Some(command_label(shown));
    }
    let id = start_key(name)?;
    let shown = items
        .iter()
        .find(|item| item.id == id)
        .map_or_else(|| id.to_string(), |item| item.title.clone());
    Some(start_label(&shown))
}

/// Whether `name` is an own command's or a Start menu entry's key whose command or entry is none
/// of `commands` and `items`: deleted since the key was added, or of an import.
pub fn missing(name: &str, commands: &[UserCommand], items: &[StartItem]) -> bool {
    if let Some(own) = own_command_key(name) {
        return !commands.iter().any(|command| command.name == own);
    }
    start_key(name).is_some_and(|id| {
        !items
            .iter()
            .any(|item| item.id == id && item.kind == StartItemKind::Item)
    })
}

/// The command a choice of the menu bar's entry `id` runs: a Start menu entry's; `None` for any
/// other id.
pub fn chosen(id: &str) -> Option<Command> {
    Some(Command::RunStartItem {
        id: start_key(id)?,
        panel: None,
        params: None,
    })
}

/// `commands` and the entries of `items` as rows of the palette among `entries`, all sorted by
/// label again: one with a key of its own names that key's row, every other one is a row of its
/// own that runs `usercmd.run` or `start_menu.run`.
pub fn add_rows(entries: &mut Vec<Entry>, commands: &[UserCommand], items: &[StartItem]) {
    let own = commands.iter().map(|command| {
        let run = Command::RunUserCommand {
            name: command.name.clone(),
            panel: None,
            params: None,
        };
        (
            command.name.clone(),
            command_label(shown_name(command)),
            run,
        )
    });
    let start = items
        .iter()
        .filter(|item| item.kind == StartItemKind::Item)
        .map(|item| {
            let run = Command::RunStartItem {
                id: item.id,
                panel: None,
                params: None,
            };
            (start_key_name(item.id), start_label(&item.title), run)
        });
    for (name, label, run) in own.chain(start) {
        if let Some(entry) = entries.iter_mut().find(|entry| entry.name == name) {
            entry.label = label;
            continue;
        }
        entries.push(Entry {
            name,
            label,
            keys: String::new(),
            chords: String::new(),
            action: Action::Run(Box::new(run)),
            current: false,
        });
    }
    entries.sort_by_key(|entry| entry.label.to_lowercase());
}

/// The entries the menu `menu` ends with: for the Start menu its entries with their submenus and
/// separators; none for any other menu.
pub fn menu_items(menu: &str, keymap: &Keymap) -> Vec<BarItem> {
    if menu != MENU_ID {
        return Vec::new();
    }
    menu_items_of(&known::start_menu(), keymap)
}

/// `items` - in menu order with their depths - as the menu's entries: each entry its title and
/// its key from `keymap` after a tab, each submenu its rows, no separator first, last or twice.
pub fn menu_items_of(items: &[StartItem], keymap: &Keymap) -> Vec<BarItem> {
    let mut at = 0;
    nest(items, &mut at, 0, keymap)
}

/// The rows from `at` on at `depth`, each submenu with the deeper rows after it, up to the first
/// row less deep.
fn nest(items: &[StartItem], at: &mut usize, depth: usize, keymap: &Keymap) -> Vec<BarItem> {
    let mut built = Vec::new();
    while let Some(item) = items.get(*at) {
        if item.depth < depth {
            break;
        }
        *at += 1;
        if item.depth > depth {
            // A row whose submenu is not listed before it: left out.
            continue;
        }
        match item.kind {
            StartItemKind::Separator => {
                if built.last().is_some_and(|last| *last != BarItem::Separator) {
                    built.push(BarItem::Separator);
                }
            }
            StartItemKind::Submenu => {
                let rows = nest(items, at, depth + 1, keymap);
                built.push(BarItem::Submenu {
                    text: item.title.clone(),
                    items: rows,
                });
            }
            StartItemKind::Item => {
                let name = start_key_name(item.id);
                let text = match super::menu_bar::key_of(keymap, &name) {
                    Some(key) => format!("{}\t{key}", item.title),
                    None => item.title.clone(),
                };
                built.push(BarItem::Entry {
                    id: interned(name),
                    text,
                });
            }
        }
    }
    if built.last() == Some(&BarItem::Separator) {
        built.pop();
    }
    built
}

#[cfg(test)]
#[path = "user_commands_tests.rs"]
mod tests;
