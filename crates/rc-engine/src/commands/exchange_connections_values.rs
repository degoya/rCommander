//! The values an import's connections part compares and writes (RC-080-19): names and texts as
//! the tree stores them, a connection's server, its change, and the ids of a script's secrets.

use std::collections::HashMap;

use crate::{
    Connection, ConnectionDraft, ConnectionPatch, FolderId, FolderNode, LoginScriptInfo,
    ScriptSecretRef, Step,
    exchange::ExportedTheme,
    settings::themes::{ThemesState, is_own_id},
};

/// A name as the tree compares it: trimmed, ASCII lower case.
pub(super) fn folded(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// An optional text as the repository stores it: trimmed, empty as none.
fn text(value: Option<&String>) -> Option<&str> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

/// `user@host:port` and the kind, as a row shows a connection's server.
pub(super) fn endpoint(draft: &ConnectionDraft) -> String {
    let port = draft.port.unwrap_or(i64::from(draft.kind.default_port()));
    match text(draft.username.as_ref()) {
        Some(user) => format!(
            "{} {user}@{}:{port}",
            draft.kind.as_str(),
            draft.host.trim()
        ),
        None => format!("{} {}:{port}", draft.kind.as_str(), draft.host.trim()),
    }
}

/// Whether `connection` has the values of `draft`; its folder and script are not compared.
pub(super) fn same(connection: &Connection, draft: &ConnectionDraft) -> bool {
    let ftp = connection.kind.is_ftp();
    connection.name == draft.name.trim()
        && endpoint(&super::super::exchange_export::draft_of(connection)) == endpoint(draft)
        && connection.auth_method == draft.auth_method
        && text(connection.key_path.as_ref()) == text(draft.key_path.as_ref())
        && connection.default_view == draft.default_view
        && text(connection.start_directory.as_ref()) == text(draft.start_directory.as_ref())
        && text(connection.notes.as_ref()) == text(draft.notes.as_ref())
        && text(connection.theme_light.as_ref()) == text(draft.theme_light.as_ref())
        && text(connection.theme_dark.as_ref()) == text(draft.theme_dark.as_ref())
        && (!ftp
            || (connection.ftp_data_mode == draft.ftp_data_mode
                && connection.ftp_encoding == draft.ftp_encoding))
}

/// The change that gives a connection the values of `draft`; an empty text clears.
pub(super) fn patch_of(draft: &ConnectionDraft) -> ConnectionPatch {
    let clear = |value: &Option<String>| Some(value.clone().unwrap_or_default());
    ConnectionPatch {
        name: Some(draft.name.clone()),
        kind: Some(draft.kind),
        host: Some(draft.host.clone()),
        port: draft.port,
        username: clear(&draft.username),
        auth_method: Some(draft.auth_method),
        key_path: clear(&draft.key_path),
        default_view: Some(draft.default_view),
        start_directory: clear(&draft.start_directory),
        notes: clear(&draft.notes),
        ftp_data_mode: draft.ftp_data_mode,
        ftp_encoding: draft.ftp_encoding,
        theme_light: clear(&draft.theme_light),
        theme_dark: clear(&draft.theme_dark),
    }
}

/// `draft` without a theme that is neither here nor among the file's (`incoming`, the themes a
/// dry run has not written yet) for its mode (RC-100-51), and whether one was left out: the
/// connection comes in without it, with a hint.
pub(super) fn fit_themes(
    draft: &ConnectionDraft,
    here: &ThemesState,
    incoming: &[ExportedTheme],
) -> (ConnectionDraft, bool) {
    let known = |id: &str, dark: bool| {
        here.theme(id).is_some_and(|theme| theme.dark == dark)
            || incoming
                .iter()
                .any(|theme| theme.id == id && theme.dark == dark && is_own_id(id))
    };
    let mut fitted = draft.clone();
    let mut dropped = false;
    for (slot, dark) in [
        (&mut fitted.theme_light, false),
        (&mut fitted.theme_dark, true),
    ] {
        let id = text(slot.as_ref()).map(str::to_owned);
        match id {
            Some(id) if !known(&id, dark) => {
                *slot = None;
                dropped = true;
            }
            Some(id) => *slot = Some(id),
            None => *slot = None,
        }
    }
    (fitted, dropped)
}

/// `steps` with each stored secret's id `from` names (by label) as `to` names it; an id with no
/// label there stays.
pub(super) fn renamed(
    steps: &[Step],
    from: &HashMap<i64, String>,
    to: &HashMap<String, i64>,
) -> Vec<Step> {
    steps
        .iter()
        .map(|step| match step {
            Step::SendSecret {
                source: ScriptSecretRef::Stored { id },
                newline,
            } => {
                let id = from
                    .get(id)
                    .and_then(|label| to.get(&folded(label)))
                    .copied()
                    .unwrap_or(*id);
                Step::SendSecret {
                    source: ScriptSecretRef::Stored { id },
                    newline: *newline,
                }
            }
            other => other.clone(),
        })
        .collect()
}

/// A script's secrets by label (folded) to their ids.
pub(super) fn ids_by_label(script: &LoginScriptInfo) -> HashMap<String, i64> {
    script
        .secrets
        .iter()
        .map(|secret| (folded(&secret.label), secret.id))
        .collect()
}

/// Every connection of the tree with its folder path (folded), and every folder by that path.
pub(super) fn walk(
    nodes: &[FolderNode],
    above: &[String],
    connections: &mut Vec<(Vec<String>, Connection)>,
    folders: &mut HashMap<Vec<String>, FolderId>,
) {
    for node in nodes {
        let mut path = above.to_vec();
        path.push(folded(&node.folder.name));
        folders.insert(path.clone(), node.folder.id);
        for connection in &node.connections {
            connections.push((path.clone(), connection.clone()));
        }
        walk(&node.folders, &path, connections, folders);
    }
}
