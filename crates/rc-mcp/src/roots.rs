//! The panels' folders as the agent's ground (`mcp.ask.outside_panels`, the audit's H2): with
//! the setting on, a reading or writing tool whose path argument leads outside the folders the
//! two panels show asks the person first, as a destructive tool does - whatever the deny lists
//! of `guard.rs` and `guard_write.rs` know or miss.
//!
//! The paths are those [`crate::unc::PATH_ARGUMENTS`] names, resolved against the call's panel
//! as the commands resolve them, then judged where they really lead (links and junctions
//! followed), so neither `..` nor a link inside a panel's folder leaves it unasked. Only this
//! machine's disks are judged: a server's or a device's path is the connection's business.

use std::path::{Path, PathBuf};

use rc_engine::{BackendId, Engine, PanelId, VfsPath};
use rmcp::model::JsonObject;
use serde_json::Value;

use crate::unc::PATH_ARGUMENTS;

/// Whether a path argument of the call leads outside both panels' folders on this machine.
pub(crate) async fn outside_panels(engine: &Engine, arguments: Option<&JsonObject>) -> bool {
    let Some(arguments) = arguments else {
        return false;
    };
    let panel = arguments
        .get("panel")
        .and_then(Value::as_str)
        .and_then(panel_of);
    let mut named = Vec::new();
    for (name, value) in arguments {
        if PATH_ARGUMENTS.contains(&name.as_str()) {
            texts(value, &mut named);
        }
    }
    if named.is_empty() {
        return false;
    }
    let mut roots = Vec::new();
    for id in [PanelId::Left, PanelId::Right] {
        if let Some(root) = on_this_machine(engine, &engine.resolve_target(Some(id), "")).await {
            roots.push(root);
        }
    }
    for text in named {
        let target = engine.resolve_target(panel, &text);
        let Some(real) = on_this_machine(engine, &target).await else {
            continue;
        };
        if !roots.iter().any(|root| real.starts_with(root)) {
            return true;
        }
    }
    false
}

/// `path` as it really is when it lies on this machine's disks (inside an archive: the archive
/// file); `None` on a server or a device.
async fn on_this_machine(engine: &Engine, path: &VfsPath) -> Option<PathBuf> {
    let mut at = path.clone();
    while let BackendId::Archive { outer, .. } = &at.backend {
        at = (**outer).clone();
    }
    if at.backend != BackendId::Local {
        return None;
    }
    // A path not there yet (a copy's target, a new folder) is judged by its nearest existing
    // folder, as it really is, and the rest of its name.
    let mut existing = at.path.clone();
    let mut rest = Vec::new();
    loop {
        let probe = VfsPath {
            backend: BackendId::Local,
            path: existing.clone(),
        };
        if let Some(real) = engine.link_target(&probe).await {
            return Some(
                rest.iter()
                    .rev()
                    .fold(real, |path: PathBuf, name| path.join(name)),
            );
        }
        let name = existing.file_name()?.to_owned();
        rest.push(name);
        if !existing.pop() || existing == Path::new("") {
            return Some(at.path);
        }
    }
}

fn panel_of(text: &str) -> Option<PanelId> {
    match text {
        "left" => Some(PanelId::Left),
        "right" => Some(PanelId::Right),
        _ => None,
    }
}

/// `value` - a text, or a list of them - into `found`.
fn texts(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::String(text) if !text.is_empty() => found.push(text.clone()),
        Value::Array(items) => {
            for item in items {
                texts(item, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;
