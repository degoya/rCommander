//! Total Commander's own commands and Start menu read into plain values (RC-100-86): the
//! sections `em_*` of `usercmd.ini`, which Total Commander keeps beside `wincmd.ini`, and the
//! `[user]` section of `wincmd.ini`. Nothing here touches a file or the database;
//! `total_commander_commands_apply` plans and writes.
//!
//! - `cmd` is a program, one of Total Commander's internal commands `cm_*`, a list of them
//!   separated by commas (internal rows: a `cm_*` name becomes rCommander's row through
//!   `tc_names`, an unknown one stays as written and is reported), or another own command
//!   `em_*`. `*` in front of `cmd` - or `*` alone with the program as the first word of `param` -
//!   starts the program as administrator. `cd <folder>` is the hotlist's, not imported.
//! - `param` is kept as written, a leading `?` (ask before the start) included; `path` is the
//!   folder, `menu` the tooltip, `button` the symbol (`file,index`), `iconic` the window (1
//!   minimized, -1 maximized). A `%VAR%` stays as text: the engine expands it when it runs.
//! - `[user]`: `menuN`, `cmdN`, `paramN`, `pathN`, `iconicN` the same way; `-Name` opens a
//!   submenu, `--` closes it, `-` is a separator. `keyN` (a key per entry) is not imported.

use rc_db::START_MENU_MAX_DEPTH;

use super::{
    tc_ini::{Ini, Section},
    total_commander::{MAX_ITEMS, TcSkip, menu_name},
};
use crate::user_commands::{
    CommandKind, CommandRef, MAX_FIELD, OWN_PREFIX, Show, StartItemKind, UserCommandDraft,
    clean_name, tc_names,
};

/// Total Commander's file of own commands, beside `wincmd.ini`.
pub const USERCMD_INI: &str = "usercmd.ini";

/// One section `em_*` of `usercmd.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcCommand {
    /// The section's name as written.
    pub section: String,
    /// What rCommander stores, or why not.
    pub draft: Result<UserCommandDraft, TcSkip>,
    /// The `cm_*` names rCommander does not know, kept in the rows as written.
    pub unknown: Vec<String>,
}

/// One row of the Start menu of `[user]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcStartEntry {
    /// `N` of `menuN`.
    pub index: usize,
    /// What the menu shows, without its `&` accelerators; empty for a separator.
    pub title: String,
    /// The submenus it is in, outermost first.
    pub folders: Vec<String>,
    pub kind: StartItemKind,
    /// An entry's command; `None` for a submenu, a separator or a skipped row.
    pub action: Option<CommandRef>,
    /// Why it is not imported; a skipped submenu skips what it holds.
    pub skip: Option<TcSkip>,
    pub unknown: Vec<String>,
}

/// `raw` without the quotes around it.
fn unquote(raw: &str) -> &str {
    let raw = raw.trim();
    raw.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map_or(raw, str::trim)
}

/// The first word of `text` (a quoted one whole) and the rest.
fn first_word(text: &str) -> (String, String) {
    let text = text.trim();
    if let Some(quoted) = text.strip_prefix('"') {
        return match quoted.split_once('"') {
            Some((word, rest)) => (word.trim().to_owned(), rest.trim().to_owned()),
            None => (quoted.trim().to_owned(), String::new()),
        };
    }
    match text.split_once(char::is_whitespace) {
        Some((word, rest)) => (word.to_owned(), rest.trim().to_owned()),
        None => (text.to_owned(), String::new()),
    }
}

/// The window `iconic` names: 1 minimized, -1 maximized, else normal.
fn show_of(iconic: Option<&str>) -> Show {
    match iconic.map(str::trim) {
        Some("1") => Show::Minimized,
        Some("-1") => Show::Maximized,
        _ => Show::Normal,
    }
}

/// The symbol `button` names: `file,index`, or a file alone (index 0).
pub(crate) fn icon_of(button: Option<&str>) -> (String, i64) {
    let button = button.map(unquote).unwrap_or_default();
    match button.rsplit_once(',') {
        Some((file, index)) => match index.trim().parse::<i64>() {
            Ok(index) => (unquote(file).to_owned(), index),
            Err(_) => (button.to_owned(), 0),
        },
        None => (button.to_owned(), 0),
    }
}

/// The command `cmd` with its `param`, `path` and `iconic` as rCommander keeps it, checked, and
/// the `cm_*` names rCommander does not know.
///
/// # Errors
///
/// The [`TcSkip`] why it is not imported: no command, a folder change, or one the checks refuse.
pub fn action_of(
    cmd: &str,
    param: &str,
    path: &str,
    iconic: Option<&str>,
) -> Result<(CommandRef, Vec<String>), TcSkip> {
    let cmd = cmd.trim();
    let (elevated, cmd) = match cmd.strip_prefix('*') {
        Some(rest) => (true, rest.trim()),
        None => (false, cmd),
    };
    let mut program = unquote(cmd).to_owned();
    let mut params = param.trim().to_owned();
    if program.is_empty() {
        // `*` alone: the program is the first word of the parameters.
        if !elevated {
            return Err(TcSkip::NoCommand);
        }
        (program, params) = first_word(&params);
        if program.is_empty() {
            return Err(TcSkip::NoCommand);
        }
    }
    let lower = program.to_ascii_lowercase();
    if lower == "cd" || lower.starts_with("cd ") {
        return Err(TcSkip::ChangeDir);
    }
    let mut unknown = Vec::new();
    let action =
        if lower.starts_with("cm_") || (lower.starts_with(OWN_PREFIX) && lower.contains(',')) {
            let mut rows = Vec::new();
            for row in program
                .split(',')
                .map(str::trim)
                .filter(|row| !row.is_empty())
            {
                let lower = row.to_ascii_lowercase();
                if lower.starts_with("cm_") {
                    match tc_names::row_of(row) {
                        Some(known) => rows.push(known.to_owned()),
                        None => {
                            unknown.push(row.to_owned());
                            rows.push(row.to_owned());
                        }
                    }
                } else if lower.starts_with(OWN_PREFIX) {
                    rows.push(lower);
                } else {
                    // A program among internal commands: Total Commander runs none either.
                    return Err(TcSkip::CommandInvalid);
                }
            }
            CommandRef {
                kind: CommandKind::Internal,
                command: rows.join(","),
                ..CommandRef::default()
            }
        } else if lower.starts_with(OWN_PREFIX) {
            CommandRef {
                kind: CommandKind::Own,
                command: lower,
                ..CommandRef::default()
            }
        } else {
            CommandRef {
                kind: CommandKind::Program,
                command: program,
                params,
                folder: unquote(path).to_owned(),
                show: show_of(iconic),
                elevated,
            }
        };
    let action = action.checked().map_err(|_| TcSkip::CommandInvalid)?;
    Ok((action, unknown))
}

/// Whether `text` fits a field: no control character, at most [`MAX_FIELD`] characters.
fn fits(text: &str) -> bool {
    !text.chars().any(char::is_control) && text.chars().count() <= MAX_FIELD
}

/// One section `em_*` read.
fn command_of(section: &Section) -> TcCommand {
    let mut unknown = Vec::new();
    let draft = clean_name(&section.name)
        .map_err(|_| TcSkip::NameInvalid)
        .and_then(|name| {
            let (action, rows) = action_of(
                section.get("cmd").unwrap_or_default(),
                section.get("param").unwrap_or_default(),
                section.get("path").unwrap_or_default(),
                section.get("iconic"),
            )?;
            unknown = rows;
            let tooltip = section.get("menu").unwrap_or_default().trim().to_owned();
            let (icon_path, icon_index) = icon_of(section.get("button"));
            if !fits(&tooltip) || !fits(&icon_path) {
                return Err(TcSkip::CommandInvalid);
            }
            Ok(UserCommandDraft {
                name,
                action,
                tooltip,
                icon_path,
                icon_index,
            })
        });
    TcCommand {
        section: section.name.clone(),
        draft,
        unknown,
    }
}

/// The own commands of `usercmd.ini`, in file order.
#[must_use]
pub fn commands(ini: &Ini) -> Vec<TcCommand> {
    ini.sections()
        .filter(|section| {
            section
                .name
                .trim()
                .to_ascii_lowercase()
                .starts_with(OWN_PREFIX)
        })
        .take(MAX_ITEMS)
        .map(command_of)
        .collect()
}

/// The Start menu of `wincmd.ini [user]`, in menu order.
#[must_use]
pub fn start_menu(ini: &Ini) -> Vec<TcStartEntry> {
    let Some(menu) = ini.section("user") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    // The open submenus, each with why it is skipped: what it holds is skipped alike.
    let mut folders: Vec<(String, Option<TcSkip>)> = Vec::new();
    for index in 1..=MAX_ITEMS {
        let Some(raw) = menu.get(&format!("menu{index}")) else {
            break;
        };
        let raw = raw.trim();
        if raw == "--" {
            folders.pop();
            continue;
        }
        let inherited = folders.iter().find_map(|(_, skip)| *skip);
        let nested: Vec<String> = folders.iter().map(|(name, _)| name.clone()).collect();
        let (kind, title) = match raw.strip_prefix('-') {
            _ if raw == "-" => (StartItemKind::Separator, String::new()),
            Some(name) => (StartItemKind::Submenu, menu_name(name)),
            None => (StartItemKind::Item, menu_name(raw)),
        };
        let mut skip = inherited;
        let named = kind == StartItemKind::Separator || (!title.is_empty() && fits(&title));
        if skip.is_none() && !named {
            skip = Some(TcSkip::NameInvalid);
        }
        // The row's level below the Start menu's import submenu.
        if skip.is_none() && nested.len() + 1 >= START_MENU_MAX_DEPTH {
            skip = Some(TcSkip::TooDeep);
        }
        let mut action = None;
        let mut unknown = Vec::new();
        if skip.is_none() && kind == StartItemKind::Item {
            let value = |key: &str| menu.get(&format!("{key}{index}"));
            match action_of(
                value("cmd").unwrap_or_default(),
                value("param").unwrap_or_default(),
                value("path").unwrap_or_default(),
                value("iconic"),
            ) {
                Ok((found, rows)) => {
                    action = Some(found);
                    unknown = rows;
                }
                Err(why) => skip = Some(why),
            }
        }
        if kind == StartItemKind::Submenu {
            folders.push((title.clone(), skip));
        }
        entries.push(TcStartEntry {
            index,
            title,
            folders: nested,
            kind,
            action,
            skip,
            unknown,
        });
    }
    entries
}

/// Whether `[user]` gives an entry a key (`keyN`), which the import leaves out.
#[must_use]
pub fn start_menu_has_keys(ini: &Ini) -> bool {
    ini.section("user").is_some_and(|menu| {
        menu.entries().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            key.strip_prefix("key")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()))
                && !value.trim().is_empty()
        })
    })
}

#[cfg(test)]
#[path = "total_commander_commands_tests.rs"]
mod tests;
