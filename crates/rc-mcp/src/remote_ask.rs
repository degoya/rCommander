//! A new host and a copy onto a server ask a person (RC-110-59).
//!
//! The default policy lets reading and writing run unasked, so an agent could save an anonymous
//! FTP connection to any host with `connections_save` and copy files there with `ops_copy`: data
//! out of the machine with nobody seeing it (the audit of 2026-11-25, Security #3). Where the
//! write rule would let such a call run, the gate asks instead ([`remote`]):
//! - `connections_save` for a host and port no stored connection has yet (`tools::new_host`);
//! - a copy or move, a pack or an unpack into a server's or a portable device's folder - written
//!   as `sftp://`, `ftp://`, `ftps://` or `device://`, or a folder below the other panel while it
//!   shows one - and a synchronisation with a side there.
//!
//! A copy from a server to this machine and one between two local folders run as before. The
//! question names the server (`remote_host`) and, for a copy, how many entries go (`entries`,
//! counted in the call as it runs, its selection frozen, `freeze.rs`); "allow for this session"
//! is bound to that server, so a grant for one host is none for another. Nothing here connects:
//! a location is read from what the call writes and what the panels show, a server's address
//! from its stored connection.

use std::path::{Component, Path};

use rc_db::Db;
use rc_engine::{BackendId, ConnectionId, Engine, McpArgument, PanelId};
use serde_json::{Map, Value};

use crate::server::McpContext;

/// The argument the question shows the server under.
pub(crate) const HOST_ARGUMENT: &str = "remote_host";

/// The argument the question shows the number of entries under.
pub(crate) const ENTRIES_ARGUMENT: &str = "entries";

/// The locations a call can name, as the engine reads them (`Engine::locate`).
const SCHEMES: [&str; 4] = ["sftp://", "ftp://", "ftps://", "device://"];

/// The server a call writes to or a new host it saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Remote {
    /// As the question shows it: the location's connection and its address, or the device.
    pub(crate) host: String,
}

impl Remote {
    /// The target "allow for this session" is bound to.
    pub(crate) fn grant(&self) -> String {
        format!("{HOST_ARGUMENT}={}", self.host)
    }
}

/// The panel a destination is read against when the call names none or a relative one.
#[derive(Debug, Clone, Copy)]
enum Side {
    /// The panel other than the call's (`panel`, the active one when omitted).
    Other,
    Left,
    Right,
}

/// The arguments of `tool` that name where it writes, each with the panel it is read against.
fn destinations(tool: &str) -> &'static [(&'static str, Side)] {
    match tool {
        "ops_copy" | "ops_move" | "archives_extract" | "archives_pack" => {
            &[("target", Side::Other)]
        }
        "dirs_sync_apply" => &[("left", Side::Left), ("right", Side::Right)],
        _ => &[],
    }
}

/// The argument whose entries a copy of `tool` sends.
fn names_argument(tool: &str) -> Option<&'static str> {
    match tool {
        "ops_copy" | "ops_move" | "archives_pack" => Some("sources"),
        "archives_extract" => Some("archives"),
        _ => None,
    }
}

/// The server a call of `tool` writes to, or the new host it saves; `None` for any other call.
pub(crate) async fn remote(
    context: &McpContext,
    tool: &str,
    arguments: Option<&Map<String, Value>>,
) -> Option<Remote> {
    if tool == crate::tools::SAVE_TOOL {
        let host = crate::tools::new_host(&context.db, arguments).await?;
        return Some(Remote { host });
    }
    let destinations = destinations(tool);
    if destinations.is_empty() {
        return None;
    }
    let engine = &context.engine;
    let panel = arguments
        .and_then(|arguments| arguments.get("panel"))
        .and_then(|panel| serde_json::from_value::<PanelId>(panel.clone()).ok())
        .unwrap_or_else(|| engine.active_panel());
    for (argument, side) in destinations {
        let read_against = match side {
            Side::Other => panel.other(),
            Side::Left => PanelId::Left,
            Side::Right => PanelId::Right,
        };
        let written = arguments
            .and_then(|arguments| arguments.get(*argument))
            .and_then(Value::as_str);
        if let Some(host) = server_of(engine, &context.db, read_against, written).await {
            return Some(Remote { host });
        }
    }
    None
}

/// The server a destination `written` (none: the panel's folder) lies on, read against `panel`.
async fn server_of(
    engine: &Engine,
    db: &Db,
    panel: PanelId,
    written: Option<&str>,
) -> Option<String> {
    if let Some((scheme, name)) = written.and_then(scheme_of) {
        return Some(named_server(db, scheme, name).await);
    }
    let folder = engine.mcp_folder(panel);
    // A drive letter leaves a server's folder for this machine (`resolve_path`).
    if written.is_some_and(names_a_drive) && !matches!(folder.backend, BackendId::Archive { .. }) {
        return None;
    }
    backend_server(db, &folder.backend).await
}

/// The scheme of a written location and the name after it; `None` for a path.
fn scheme_of(written: &str) -> Option<(&'static str, &str)> {
    SCHEMES.into_iter().find_map(|scheme| {
        let head = written.get(..scheme.len())?;
        let rest = written.get(scheme.len()..)?;
        head.eq_ignore_ascii_case(scheme)
            .then(|| (scheme, rest.split_once('/').map_or(rest, |(name, _)| name)))
    })
}

/// Whether `written` starts with a drive (`C:`), as a Windows path can.
fn names_a_drive(written: &str) -> bool {
    matches!(
        Path::new(written).components().next(),
        Some(Component::Prefix(_))
    )
}

/// The server a written location names: its scheme and name, with the address of the one
/// connection of that name and protocol where there is exactly one.
async fn named_server(db: &Db, scheme: &str, name: &str) -> String {
    let shown = format!("{scheme}{name}");
    if scheme == "device://" {
        return shown;
    }
    let sftp = scheme == "sftp://";
    let found: Vec<_> = db
        .connections_named(name)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|connection| connection.kind.is_ftp() != sftp)
        .collect();
    match found.as_slice() {
        [connection] => format!("{shown} ({})", address(&connection.host, connection.port)),
        _ => shown,
    }
}

/// The server a folder's backend lies on: an archive's inside where its file lies.
async fn backend_server(db: &Db, backend: &BackendId) -> Option<String> {
    let mut backend = backend;
    while let BackendId::Archive { outer, .. } = backend {
        backend = &outer.backend;
    }
    match backend {
        BackendId::Local => None,
        BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } => {
            Some(stored_server(db, *connection_id).await)
        }
        BackendId::Device { device } => Some(format!("device://{device}")),
        // A file system this build does not name here is held to be another machine's.
        _ => Some("another file system".to_owned()),
    }
}

/// A stored connection as the question names it.
async fn stored_server(db: &Db, id: i64) -> String {
    match db.connection(ConnectionId(id)).await {
        Ok(connection) => {
            let scheme = if connection.kind.is_ftp() {
                "ftp://"
            } else {
                "sftp://"
            };
            format!(
                "{scheme}{} ({})",
                connection.name,
                address(&connection.host, connection.port)
            )
        }
        Err(_) => format!("connection {id}"),
    }
}

/// `host:port`, an IPv6 address in brackets.
pub(crate) fn address(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// What the question shows besides the call's arguments: the server and, for a copy, how many
/// entries go - counted in `run`, the arguments the call runs with. Masked and made readable as
/// every shown argument is (`ask::shown`).
pub(crate) fn shown(
    remote: Option<&Remote>,
    tool: &str,
    run: Option<&Map<String, Value>>,
    engine: &Engine,
) -> Vec<McpArgument> {
    let Some(remote) = remote else {
        return Vec::new();
    };
    let mut shown = Map::new();
    shown.insert(HOST_ARGUMENT.to_owned(), Value::from(remote.host.clone()));
    if let Some(count) = names_argument(tool)
        .and_then(|names| run?.get(names))
        .and_then(Value::as_array)
        .map(Vec::len)
    {
        shown.insert(ENTRIES_ARGUMENT.to_owned(), Value::from(count));
    }
    crate::ask::shown(Some(&shown), engine)
}

#[cfg(test)]
#[path = "remote_ask_tests.rs"]
mod tests;
