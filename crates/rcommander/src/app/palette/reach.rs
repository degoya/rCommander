//! How the window reaches the registry commands that have no key (RC-080-05), kept as data in
//! `reach.json` beside this file: one row per command, which the command palette and the
//! keyboard reference (`scripts/keyboard-reference.sh`) both read.
//!
//! A row says one of: `palette` - the command palette runs it without parameters; `via` - it is
//! reached in what another command opens (`"via": "connections.manager"`, with `how`), or in a
//! place of the window (`"via": "window:settings"`: a window key of the keymap, or one of
//! `SURFACES`); `asked` - a question the window opens by itself runs it; `shown` - it is a
//! read-out of what the window shows anyway; `omitted` - no key reaches it, and why. A command
//! with a key has no row. `problems` (tests) holds the rows against the registry and the keymap, so a
//! new command without a key and without a row fails its test.

#[cfg(test)]
use std::collections::HashSet;

use rc_engine::Command;
#[cfg(test)]
use rc_engine::Keymap;
use serde::Deserialize;
use serde_json::json;

const REACH: &str = include_str!("reach.json");

/// The places of the window a `via` names as `window:<place>` besides the keymap's window keys:
/// the settings and the database dialog (both in the command palette) and the command line.
#[cfg(test)]
pub const SURFACES: &[&str] = &["settings", "database", "cmdline"];

/// How a command without a key is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    Palette,
    Via { target: String, how: String },
    Asked(String),
    Shown(String),
    Omitted(String),
}

/// One row of `reach.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachRow {
    pub command: String,
    pub reach: Reach,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    command: String,
    #[serde(default)]
    palette: bool,
    #[serde(default)]
    via: Option<String>,
    #[serde(default)]
    how: Option<String>,
    #[serde(default)]
    asked: Option<String>,
    #[serde(default)]
    shown: Option<String>,
    #[serde(default)]
    omitted: Option<String>,
}

/// The rows of `text`.
///
/// # Errors
///
/// The text is no list of rows, or a row says none or more than one of the kinds, or a `via`
/// without `how` (or `how` without `via`).
pub fn parse(text: &str) -> Result<Vec<ReachRow>, String> {
    let raw: Vec<RawRow> = serde_json::from_str(text).map_err(|error| error.to_string())?;
    raw.into_iter()
        .map(|row| {
            let reach = match (
                row.palette,
                row.via,
                row.how,
                row.asked,
                row.shown,
                row.omitted,
            ) {
                (true, None, None, None, None, None) => Reach::Palette,
                (false, Some(target), Some(how), None, None, None) => Reach::Via { target, how },
                (false, None, None, Some(text), None, None) => Reach::Asked(text),
                (false, None, None, None, Some(text), None) => Reach::Shown(text),
                (false, None, None, None, None, Some(text)) => Reach::Omitted(text),
                _ => return Err(format!("{}: say one way it is reached", row.command)),
            };
            Ok(ReachRow {
                command: row.command,
                reach,
            })
        })
        .collect()
}

/// The rows of `reach.json`.
pub fn standard() -> Vec<ReachRow> {
    parse(REACH).expect("reach.json reads (reach_tests)")
}

/// The command a palette row runs: `id` without parameters.
pub fn palette_command(id: &str) -> Option<Command> {
    serde_json::from_value(json!({ "command": id, "params": {} }))
        .or_else(|_| serde_json::from_value(json!({ "command": id })))
        .ok()
}

/// What is wrong with `rows` for the registry's `ids` and `keymap`, one line each: a command
/// without a key and without a row, a row for a command with a key or for no command, a row
/// twice, a palette row that does not build its command, a `via` that leads nowhere reachable,
/// a reason too short to be one.
#[cfg(test)]
pub fn problems(ids: &[&str], keymap: &Keymap, rows: &[ReachRow]) -> Vec<String> {
    let bound: HashSet<&str> = keymap
        .bindings()
        .iter()
        .map(|binding| binding.command.id())
        .collect();
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let id = row.command.as_str();
        if !seen.insert(id) {
            problems.push(format!("{id}: two rows"));
        }
        if !ids.contains(&id) {
            problems.push(format!("{id}: not a command of the registry"));
        } else if bound.contains(id) {
            problems.push(format!("{id} has a key: drop its row from reach.json"));
        }
        match &row.reach {
            Reach::Palette if palette_command(id).is_none() => {
                problems.push(format!(
                    "{id}: the palette cannot run it without parameters"
                ));
            }
            Reach::Via { how, .. }
            | Reach::Asked(how)
            | Reach::Shown(how)
            | Reach::Omitted(how)
                if how.trim().len() < 8 =>
            {
                problems.push(format!("{id}: say how, or why not"));
            }
            _ => {}
        }
        if let Reach::Via { target, .. } = &row.reach
            && !reachable(target, keymap, rows, &bound, 0)
        {
            problems.push(format!("{id}: {target} is not reached by a key"));
        }
    }
    for id in ids {
        if !bound.contains(id) && !seen.contains(id) {
            problems.push(format!("{id} has no key and no row in reach.json"));
        }
    }
    problems
}

/// Whether `target` - a command or `window:<place>` - is reached from the keyboard: a bound or
/// palette command, a window key or surface, or a command reached through one of those.
#[cfg(test)]
fn reachable(
    target: &str,
    keymap: &Keymap,
    rows: &[ReachRow],
    bound: &HashSet<&str>,
    depth: usize,
) -> bool {
    if let Some(place) = target.strip_prefix("window:") {
        return SURFACES.contains(&place) || keymap.window().iter().any(|key| key.name == place);
    }
    if bound.contains(target) {
        return true;
    }
    // A chain longer than the rows is a loop.
    if depth > rows.len() {
        return false;
    }
    rows.iter()
        .filter(|row| row.command == target)
        .any(|row| match &row.reach {
            Reach::Palette => true,
            Reach::Via { target, .. } => reachable(target, keymap, rows, bound, depth + 1),
            Reach::Asked(_) | Reach::Shown(_) | Reach::Omitted(_) => false,
        })
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod tests;
