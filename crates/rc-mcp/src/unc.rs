//! Network shares named by an agent (RC-080-10): a call whose path argument reaches a share on
//! another machine is refused with `network_path` before its tool runs. Opening such a path makes
//! Windows sign in to the server with the person's NTLM hash, and the server may be anyone's.
//!
//! Every tool is judged by its argument names ([`PATH_ARGUMENTS`]), so a new tool with a `path`
//! is covered without a line of its own; `settings_set` also by its value when the key is a path
//! setting (the start folders). A WSL distribution's share is no network share
//! ([`rc_engine::network_path`]), and the share of a mapped network drive is one the person
//! connected already (RC-010-16): both pass. What the person types or picks in the window never
//! comes here.

use rc_engine::{
    Command, Drive, DriveKind, Engine, NetworkPath, Outcome, network_path,
    settings::{SettingKind, setting},
};
use rmcp::model::{CallToolResult, JsonObject};
use serde_json::Value;

use crate::answer::error_result;

/// The arguments of every tool that name a file or folder on this machine.
const PATH_ARGUMENTS: &[&str] = &[
    "archives",
    "cwd",
    "dir",
    "ftp_ini",
    "key_path",
    "left",
    "link",
    "path",
    "paths",
    "right",
    "root",
    "sources",
    "target",
    "target_dir",
    "wincmd_ini",
];

/// The refusal for a call of `tool` with `arguments` when one of its paths reaches a share on
/// another machine that no mapped network drive reaches.
pub(crate) async fn refuse_network(
    engine: &Engine,
    tool: &str,
    arguments: Option<&JsonObject>,
) -> Option<CallToolResult> {
    let named = network_arguments(tool, arguments?);
    if named.is_empty() {
        return None;
    }
    let text = match refused_whatever_is_mapped(&named) {
        Some(text) => text.to_owned(),
        None => {
            // Asked only now: almost no call names a share.
            // Boxed: the gate's future would otherwise hold the whole dispatch inline.
            let drives =
                match Box::pin(engine.dispatch_mcp(Command::ListDrives { refresh: false })).await {
                    Ok(Outcome::Drives { drives }) => drives,
                    _ => Vec::new(),
                };
            let (text, _) = named.into_iter().find(|(_, network)| match network {
                NetworkPath::Share(share) => !mapped(share, &drives),
                NetworkPath::Other => true,
            })?;
            text
        }
    };
    Some(error_result(
        "network_path",
        &format!(
            "{text} is on another machine: rCommander opens a network share for an agent only \
             through a network drive the person mapped (use its letter, as drives_list names \
             it), because Windows would sign in to that server with the person's credentials. \
             WSL distributions (\\\\wsl.localhost\\<name>) are no network shares. The person \
             can open the share in rCommander."
        ),
    ))
}

/// The first named path that no mapped drive lets through - a device or NT path, a UNC path
/// without a share ([`NetworkPath::Other`]) - so the gate answers without listing the drives
/// (RC-080-54); `None` when only shares are named and the drives decide.
fn refused_whatever_is_mapped(named: &[(String, NetworkPath)]) -> Option<&str> {
    named
        .iter()
        .find(|(_, network)| matches!(network, NetworkPath::Other))
        .map(|(text, _)| text.as_str())
}

/// Every path argument of the call that names something on another machine, with what it
/// reaches.
fn network_arguments(tool: &str, arguments: &JsonObject) -> Vec<(String, NetworkPath)> {
    let path_setting = tool == "settings_set"
        && arguments
            .get("key")
            .and_then(Value::as_str)
            .and_then(setting)
            .is_some_and(|spec| matches!(spec.kind, SettingKind::Path { .. }));
    let mut found = Vec::new();
    for (name, value) in arguments {
        if PATH_ARGUMENTS.contains(&name.as_str()) || (path_setting && name == "value") {
            judge(value, &mut found);
        }
    }
    found
}

/// `value` - a text, or a list of them - into `found` when it names something on another
/// machine.
fn judge(value: &Value, found: &mut Vec<(String, NetworkPath)>) {
    match value {
        Value::String(text) => {
            if let Some(network) = network_path(text) {
                found.push((text.clone(), network));
            }
        }
        Value::Array(items) => {
            for item in items {
                judge(item, found);
            }
        }
        _ => {}
    }
}

/// Whether a network drive is mapped to `share` (`\\server\share`, ignoring case and a trailing
/// separator).
fn mapped(share: &str, drives: &[Drive]) -> bool {
    drives.iter().any(|drive| {
        drive.kind == DriveKind::Network
            && drive.remote.as_deref().is_some_and(|remote| {
                remote
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .eq_ignore_ascii_case(share)
            })
    })
}

#[cfg(test)]
#[path = "unc_tests.rs"]
mod tests;
