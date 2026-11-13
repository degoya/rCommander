//! What the question about `workspaces_apply` shows of its workspace (RC-110-21).
//!
//! The call names an id, and "apply workspace 3" tells the person nothing: which folders the
//! panels open and which shell starts is what they allow. [`described`] reads the workspace
//! before the question and writes its name, each panel's tabs and the terminal a panel would
//! show into the arguments the question shows - in the window (under `mcp_confirm.shown.*`) and
//! in the answer that carries a headless code alike. An argument of the same name the call
//! brought is replaced, so a call cannot dress its workspace up.
//!
//! What the person reads is what runs (RC-080-34): `workspaces_update` asks nobody, so the same
//! agent could change the workspace while the question stands. In the window the workspace is
//! read again after the answer ([`changed`]) and the call refused with `workspace_changed` when
//! it no longer reads as shown; a headless code is bound to the arguments shown
//! (`confirm::subject_of`), so a code issued for other places is not redeemed.

use rc_engine::{Command, Engine, Outcome, WorkspacesOutcome};
use rmcp::model::CallToolResult;
use serde_json::{Map, Value};

use crate::answer::error_result;

/// The tool whose question names its workspace.
const TOOL: &str = "workspaces_apply";

/// Between two tabs of a panel.
const BETWEEN: &str = " | ";

/// The arguments of a `workspaces_apply` call with its workspace written out: `workspace` (the
/// name), `left_tabs` and `right_tabs`, and `left_terminal` or `right_terminal` for a panel that
/// would show a terminal. `Ok(None)` for another tool and for a call without a usable `id`,
/// which the tool refuses itself; the engine's refusal for a workspace that is not there.
pub(crate) async fn described(
    engine: &Engine,
    tool: &str,
    arguments: Option<&Map<String, Value>>,
) -> Result<Option<Map<String, Value>>, CallToolResult> {
    if tool != TOOL {
        return Ok(None);
    }
    let Some(id) = arguments
        .and_then(|arguments| arguments.get("id"))
        .and_then(Value::as_i64)
    else {
        return Ok(None);
    };
    // Boxed: the gate's future would otherwise hold the whole dispatch inline.
    let workspace = match Box::pin(engine.dispatch_mcp(Command::GetWorkspace { id })).await {
        Ok(Outcome::Workspaces(outcome)) => match *outcome {
            WorkspacesOutcome::Workspace(workspace) => workspace,
            _ => return Ok(None),
        },
        Ok(_) => return Ok(None),
        Err(error) => return Err(error_result(error.code(), &error.to_string())),
    };
    let mut shown = arguments.cloned().unwrap_or_default();
    shown.insert("workspace".to_owned(), Value::from(workspace.name.clone()));
    for (side, panel) in [
        ("left", &workspace.spec.left),
        ("right", &workspace.spec.right),
    ] {
        let tabs = panel.shown_tabs().join(BETWEEN);
        shown.insert(format!("{side}_tabs"), Value::from(tabs));
        let terminal = format!("{side}_terminal");
        match panel.shown_terminal() {
            Some(text) => shown.insert(terminal, Value::from(text)),
            None => shown.remove(&terminal),
        };
    }
    Ok(Some(shown))
}

/// The refusal of a call the person allowed as `shown` when its workspace reads otherwise by
/// now; `None` while it reads the same, and for a call [`described`] wrote nothing into.
pub(crate) async fn changed(
    engine: &Engine,
    tool: &str,
    shown: Option<&Map<String, Value>>,
) -> Option<CallToolResult> {
    let shown = shown?;
    match described(engine, tool, Some(shown)).await {
        Ok(Some(now)) if now == *shown => None,
        Ok(_) => Some(error_result(
            "workspace_changed",
            &format!(
                "the workspace of {tool} was changed while the person read the question; nothing \
                 was done. Call it again: the person is asked about the workspace as it is now"
            ),
        )),
        Err(refusal) => Some(refusal),
    }
}

#[cfg(test)]
#[path = "workspace_shown_tests.rs"]
mod tests;
