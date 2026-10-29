//! What an asked call reads of the panels, frozen before the question (RC-080-34).
//!
//! A tool that takes names from a panel when its call leaves them out - the selection, else the
//! entry under the cursor - or that works in a panel's folder used to read them only when it
//! ran: while the person read the question, the same agent could select everything
//! (`panel_select`), move the cursor (`set_cursor`) or the folder, and the person allowed
//! something other than what ran. [`freeze`] reads the panel once, before the question: the
//! panel, the names and the cursor's entry are written into the call's arguments, the question
//! shows exactly those (the names as the panel's entries, every other panel the call reads by
//! its folder), and the call runs with the folders of those panels pinned
//! (`rc_engine::with_mcp_pins`) - one standing elsewhere by then refuses it with
//! `panel_changed`.
//!
//! The same freeze holds where no window asks (RC-080-55): a headless confirmation code holds the
//! frozen call and runs it when redeemed ([`as_frozen`] binds the redeeming call to it), and a
//! call allowed for the session freezes and runs pinned as well.

use rc_engine::{Engine, McpPanelContext, McpPin, PanelId};
use rmcp::model::CallToolResult;
use serde_json::{Map, Value};

use crate::answer::error_result;

/// What a tool takes from the panels when its call leaves it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Takes {
    /// The argument the selection, else the cursor's entry, goes into as a list.
    names: Option<&'static str>,
    /// The argument the entry under the cursor goes into, whatever is selected (`file_open`).
    cursor: Option<&'static str>,
    /// The tool names its panel by `panel`, the active one when omitted.
    panel: bool,
    /// It reads the other panel's folder too: a target there, the other side of a comparison.
    other: bool,
}

/// The tools that read a panel's selection, cursor or folder when they run: every destructive
/// and executing one, and those an argument makes destructive (an overwrite, deleted sources).
fn takes(tool: &str) -> Option<Takes> {
    let names = |argument, other| Takes {
        names: Some(argument),
        cursor: None,
        panel: true,
        other,
    };
    let folder = |panel, other| Takes {
        names: None,
        cursor: None,
        panel,
        other,
    };
    Some(match tool {
        "ops_delete" => names("sources", false),
        "ops_copy" | "ops_move" | "archives_pack" => names("sources", true),
        "archives_extract" => names("archives", true),
        "file_open" => Takes {
            cursor: Some("path"),
            ..folder(true, false)
        },
        "terminal_open" => folder(true, false),
        "dirs_sync_apply" => folder(false, true),
        _ => return None,
    })
}

/// An asked call as [`freeze`] fixed it.
#[derive(Debug, Clone)]
pub(crate) struct Frozen {
    /// The arguments the tool runs with: the call's, its panel and the names it takes from it
    /// written out.
    pub(crate) arguments: Map<String, Value>,
    /// The arguments the question shows: the same without a list of names the question shows
    /// as the panel's entries, with the folder of the other panel where the call reads it
    /// (`left_panel`, `right_panel`).
    pub(crate) shown: Map<String, Value>,
    /// The panel the question shows.
    pub(crate) context: McpPanelContext,
    /// The folders of the panels the call reads.
    pub(crate) pins: Vec<McpPin>,
}

/// The call `arguments` of `tool` with what it would read of the panels written out; `Ok(None)`
/// for a tool that reads none. Refused: a panel that is neither `left` nor `right`, and a call
/// that would take names from a panel with nothing selected and nothing under the cursor - by
/// the time it ran there might be something.
pub(crate) fn freeze(
    engine: &Engine,
    tool: &str,
    arguments: Option<&Map<String, Value>>,
) -> Result<Option<Frozen>, CallToolResult> {
    let Some(takes) = takes(tool) else {
        return Ok(None);
    };
    let mut run = arguments.cloned().unwrap_or_default();
    let named = match run.get("panel").filter(|_| takes.panel) {
        None | Some(Value::Null) => None,
        Some(panel) => match serde_json::from_value::<PanelId>(panel.clone()) {
            Ok(panel) => Some(panel),
            Err(_) => {
                return Err(error_result(
                    "invalid_params",
                    &format!("panel of {tool} is left or right; nothing was done"),
                ));
            }
        },
    };
    let cursor = takes
        .cursor
        .is_some_and(|argument| run.get(argument).is_none_or(Value::is_null));
    let frozen = engine.freeze_mcp_panel(named, cursor);
    if takes.panel {
        run.insert("panel".to_owned(), Value::from(side(frozen.panel)));
    }
    let mut shown = run.clone();
    if let Some(argument) = takes.names
        && run.get(argument).is_none_or(leaves_it_to_the_panel)
    {
        if frozen.names.is_empty() {
            return Err(error_result(
                "no_sources",
                &format!(
                    "{tool} was called without {argument}, and the {} panel has nothing selected \
                     and no entry under its cursor; nothing was done. Name the entries in \
                     {argument}",
                    side(frozen.panel)
                ),
            ));
        }
        run.insert(argument.to_owned(), Value::from(frozen.names.clone()));
    }
    if let Some(argument) = takes.cursor.filter(|_| cursor) {
        let Some(name) = frozen.names.first() else {
            return Err(error_result(
                "invalid_params",
                &format!(
                    "{tool} was called without {argument}, and no entry is under the cursor of \
                     the {} panel; nothing was done. Name the entry in {argument}",
                    side(frozen.panel)
                ),
            ));
        };
        run.insert(argument.to_owned(), Value::from(name.clone()));
        shown.insert(argument.to_owned(), Value::from(name.clone()));
    }
    let mut pins = vec![McpPin {
        panel: frozen.panel,
        path: frozen.path,
    }];
    if takes.other {
        let other = frozen.panel.other();
        shown.insert(
            format!("{}_panel", side(other)),
            Value::from(frozen.other.path.display().to_string()),
        );
        pins.push(McpPin {
            panel: other,
            path: frozen.other,
        });
    }
    Ok(Some(Frozen {
        arguments: run,
        shown,
        context: frozen.context,
        pins,
    }))
}

/// The call `arguments` of a redeemed code with what `frozen` wrote out where the call leaves it
/// to the panel (missing, null or empty), the code left out: the same call as asked gives the
/// frozen arguments again, whatever the panel shows by now; another one does not.
pub(crate) fn as_frozen(
    arguments: Option<&Map<String, Value>>,
    frozen: &Frozen,
) -> Map<String, Value> {
    let mut call = arguments.cloned().unwrap_or_default();
    call.remove("confirmation");
    for (name, value) in &frozen.arguments {
        if name != "confirmation" && call.get(name).is_none_or(leaves_it_to_the_panel) {
            call.insert(name.clone(), value.clone());
        }
    }
    call
}

/// What a question without a window names of a frozen call: what the window shows, with the
/// names the panel gave written out, as the window shows them as the panel's entries instead.
pub(crate) fn shown_without_a_window(frozen: &Frozen) -> Map<String, Value> {
    let mut shown = frozen.shown.clone();
    for (name, value) in &frozen.arguments {
        if !shown.contains_key(name) {
            shown.insert(name.clone(), value.clone());
        }
    }
    shown
}

/// Whether a names argument leaves the names to the panel, as the tools read it: missing, null
/// or empty.
fn leaves_it_to_the_panel(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

/// A panel as the tools name it.
fn side(panel: PanelId) -> &'static str {
    match panel {
        PanelId::Left => "left",
        PanelId::Right => "right",
    }
}

#[cfg(test)]
#[path = "freeze_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "freeze_headless_tests.rs"]
mod headless_tests;
