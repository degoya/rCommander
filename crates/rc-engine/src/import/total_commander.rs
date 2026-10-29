//! What Total Commander's files hold, read into plain values (RC-050-09): the directory menu
//! (`[DirMenu]`), the folder tabs of both panels (`[lefttabs]`, `[righttabs]`, the active
//! folders of `[left]`, `[right]`) and the FTP connections of `wcx_ftp.ini`; the colours and the
//! font are `total_commander_look`'s (RC-050-14). Nothing here touches
//! a file, the database or a password's value; `total_commander_apply` plans and writes.
//!
//! - A menu entry `cd <folder>` is an entry, its `pathN` the folder for the other panel; a
//!   submenu (`-Name` ... `--`) is a folder, a separator (`-`) is left out. Programs, internal
//!   commands (`cm_*`), FTP and plugin paths, relative paths and unknown `%variables%` are
//!   skipped with their reason.
//! - The tabs are listed as Total Commander writes them: the inactive ones as `N_path`, the active
//!   one in `[left]`/`[right]` `path` at the place `activetab` names. A tab is locked by
//!   `N_locked` (`activelocked` for the active one) other than `0`.
//! - A connection is a section named in `[connections]`; a `|` in its name puts it into folders,
//!   as Total Commander's FTP connection list writes them (its author: the pipe separates folders
//!   and names because a backslash is often part of a name), so a backslash stays in the name. The protocol comes from the host's scheme: `sftp://` is SSH, `ftps://` FTPS
//!   (implicit on port 990), none or `ftp://` plain FTP.

use rc_db::HOTLIST_MAX_DEPTH;
use serde::{Deserialize, Serialize};

use super::{
    tc_ini::Ini,
    tc_password::{TcPassword, classify},
};
use crate::{ConnectionKind, PanelId};

/// Resolves a `%variable%`; `None` when it is unknown.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The most menu items, tabs per panel and connections read.
pub const MAX_ITEMS: usize = 2048;

/// How deep the imported menu nests below the hotlist folder it goes into.
const MAX_MENU_DEPTH: usize = HOTLIST_MAX_DEPTH - 1;

/// How deep a connection's folders nest below the import's folder.
const MAX_CONNECTION_FOLDERS: usize = 8;

/// The longest name kept, in characters (the hotlist's limit).
const MAX_NAME: usize = rc_db::HOTLIST_MAX_NAME;

/// Why an item is not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TcSkip {
    /// The entry starts a program.
    Program,
    /// The entry runs one of Total Commander's internal commands (`cm_*`).
    InternalCommand,
    /// The folder is on an FTP or other server URL.
    ServerPath,
    /// The folder is one of a file-system plugin (`\\\Plugin`).
    PluginPath,
    /// The entry has no command.
    NoCommand,
    /// The folder is not absolute.
    RelativePath,
    /// The folder names a `%variable%` that is not set.
    UnknownVariable,
    /// The folder holds characters no path has, or is empty.
    InvalidPath,
    /// It nests deeper than the hotlist or the connection tree takes.
    TooDeep,
    /// The name is empty or too long.
    NameInvalid,
    /// The host's scheme is no protocol rCommander speaks.
    ProtocolUnknown,
    /// The host's port is not a number from 1 to 65535.
    PortInvalid,
    /// An inverted cursor or selection, which rCommander does not draw (RC-050-14).
    Inverse,
    /// A colour value that is no colour.
    ColourInvalid,
    /// A rule by file type on a saved search, which rCommander does not keep.
    SearchFilter,
    /// More rules by file type than the setting takes.
    TooMany,
}

impl TcSkip {
    /// Every code, for the window's texts.
    pub const ALL: &[Self] = &[
        Self::Program,
        Self::InternalCommand,
        Self::ServerPath,
        Self::PluginPath,
        Self::NoCommand,
        Self::RelativePath,
        Self::UnknownVariable,
        Self::InvalidPath,
        Self::TooDeep,
        Self::NameInvalid,
        Self::ProtocolUnknown,
        Self::PortInvalid,
        Self::Inverse,
        Self::ColourInvalid,
        Self::SearchFilter,
        Self::TooMany,
    ];

    /// The stable code; the window's text is `tc_import.skip.<code>`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::InternalCommand => "internal_command",
            Self::ServerPath => "server_path",
            Self::PluginPath => "plugin_path",
            Self::NoCommand => "no_command",
            Self::RelativePath => "relative_path",
            Self::UnknownVariable => "unknown_variable",
            Self::InvalidPath => "invalid_path",
            Self::TooDeep => "too_deep",
            Self::NameInvalid => "name_invalid",
            Self::ProtocolUnknown => "protocol_unknown",
            Self::PortInvalid => "port_invalid",
            Self::Inverse => "inverse",
            Self::ColourInvalid => "colour_invalid",
            Self::SearchFilter => "search_filter",
            Self::TooMany => "too_many",
        }
    }
}

/// One item of the directory menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcMenuItem {
    /// `N` of `menuN`.
    pub index: usize,
    pub name: String,
    /// The submenus it is in, outermost first.
    pub folders: Vec<String>,
    pub kind: TcMenuKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcMenuKind {
    Folder,
    Entry {
        path: String,
        /// The folder for the other panel; dropped when it does not read as one.
        target: Option<String>,
    },
    Skipped(TcSkip),
}

/// One folder tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcTab {
    pub panel: PanelId,
    /// Its place in the panel's row of tabs.
    pub position: usize,
    pub path: Result<String, TcSkip>,
    pub caption: Option<String>,
    pub locked: bool,
    pub active: bool,
}

/// One connection of `wcx_ftp.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcConnection {
    /// The section's name, the connection's key in the file.
    pub section: String,
    pub name: String,
    /// The folders its name puts it in, outermost first.
    pub folders: Vec<String>,
    pub kind: Result<ConnectionKind, TcSkip>,
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub directory: Option<String>,
    /// Whether a password is there - never its value.
    pub password: TcPassword,
}

/// `text` with every `%name%` replaced through `lookup`; `None` when one is unknown.
#[must_use]
pub fn expand(text: &str, lookup: Lookup<'_>) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let name = &after[..end];
        // Parentheses for `%ProgramFiles(x86)%`.
        let allowed = |c: char| c.is_alphanumeric() || matches!(c, '_' | '(' | ')');
        if name.is_empty() || !name.chars().all(allowed) {
            return None;
        }
        out.push_str(&lookup(name)?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// A menu name without its `&` accelerators.
fn menu_name(raw: &str) -> String {
    raw.replace("&&", "\u{0}")
        .replace('&', "")
        .replace('\u{0}', "&")
        .trim()
        .to_owned()
}

/// A name, when it is not empty and not too long.
fn checked_name(name: String) -> Option<String> {
    (!name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control))
        .then_some(name)
}

/// The folder `raw` names: absolute, local or UNC, `%variables%` expanded.
pub fn folder(raw: &str, lookup: Lookup<'_>) -> Result<String, TcSkip> {
    let raw = raw.trim().trim_matches('"').trim();
    if raw.is_empty() {
        return Err(TcSkip::InvalidPath);
    }
    if raw.contains("://") {
        return Err(TcSkip::ServerPath);
    }
    if raw.starts_with("\\\\\\") {
        return Err(TcSkip::PluginPath);
    }
    let path = expand(raw, lookup).ok_or(TcSkip::UnknownVariable)?;
    if path.chars().any(char::is_control) {
        return Err(TcSkip::InvalidPath);
    }
    let bytes = path.as_bytes();
    let drive = rc_vfs::drive_letter(&path).is_some();
    if drive && bytes.len() == 2 {
        return Ok(format!("{path}\\"));
    }
    let absolute = (drive && matches!(bytes[2], b'\\' | b'/'))
        || path.starts_with("\\\\")
        || path.starts_with('/');
    if absolute {
        Ok(path)
    } else {
        Err(TcSkip::RelativePath)
    }
}

/// The directory menu of `wincmd.ini`, in menu order; separators left out.
#[must_use]
pub fn hotlist(ini: &Ini, lookup: Lookup<'_>) -> Vec<TcMenuItem> {
    let Some(menu) = ini.section("DirMenu") else {
        return Vec::new();
    };
    let mut items = Vec::new();
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
        if raw == "-" {
            continue;
        }
        let (folder_name, name) = match raw.strip_prefix('-') {
            Some(name) => (true, menu_name(name)),
            None => (false, menu_name(raw)),
        };
        let inherited = folders.iter().find_map(|(_, skip)| *skip);
        let kind = match (inherited, checked_name(name.clone())) {
            (Some(skip), _) => TcMenuKind::Skipped(skip),
            (None, None) => TcMenuKind::Skipped(TcSkip::NameInvalid),
            // The item's level below the hotlist folder the import fills.
            (None, Some(_)) if folders.len() + 1 > MAX_MENU_DEPTH => {
                TcMenuKind::Skipped(TcSkip::TooDeep)
            }
            (None, Some(_)) if folder_name => TcMenuKind::Folder,
            (None, Some(_)) => entry(
                menu.get(&format!("cmd{index}")).unwrap_or_default(),
                menu.get(&format!("path{index}")),
                lookup,
            ),
        };
        let nested = folders.iter().map(|(name, _)| name.clone()).collect();
        if folder_name {
            let skip = match &kind {
                TcMenuKind::Skipped(skip) => Some(*skip),
                _ => None,
            };
            folders.push((name.clone(), skip));
        }
        items.push(TcMenuItem {
            index,
            name,
            folders: nested,
            kind,
        });
    }
    items
}

/// A menu entry's command and target.
fn entry(command: &str, target: Option<&str>, lookup: Lookup<'_>) -> TcMenuKind {
    let command = command.trim();
    if command.is_empty() {
        return TcMenuKind::Skipped(TcSkip::NoCommand);
    }
    let lower = command.to_ascii_lowercase();
    let path = if lower == "cd" {
        ""
    } else if let Some(path) = lower.strip_prefix("cd ").map(|_| &command[3..]) {
        path
    } else if lower.starts_with("cm_") {
        return TcMenuKind::Skipped(TcSkip::InternalCommand);
    } else {
        return TcMenuKind::Skipped(TcSkip::Program);
    };
    match folder(path, lookup) {
        Ok(path) => TcMenuKind::Entry {
            path,
            target: target
                .filter(|target| !target.trim().is_empty())
                .and_then(|target| folder(target, lookup).ok()),
        },
        Err(skip) => TcMenuKind::Skipped(skip),
    }
}

/// Whether a lock value is set: present and not `0`.
pub(super) fn flag(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty() && value.trim() != "0")
}

/// The tabs of `panel`, in their order.
#[must_use]
pub fn tabs(ini: &Ini, panel: PanelId, lookup: Lookup<'_>) -> Vec<TcTab> {
    let (side, row) = match panel {
        PanelId::Left => ("left", "lefttabs"),
        PanelId::Right => ("right", "righttabs"),
    };
    let section = ini.section(row);
    let mut tabs = Vec::new();
    if let Some(section) = section {
        for index in 0..MAX_ITEMS {
            let Some(path) = section.get(&format!("{index}_path")) else {
                break;
            };
            tabs.push(TcTab {
                panel,
                position: 0,
                path: folder(path, lookup),
                caption: caption(section.get(&format!("{index}_caption"))),
                locked: flag(section.get(&format!("{index}_locked"))),
                active: false,
            });
        }
    }
    if let Some(path) = ini.get(side, "path") {
        let at = section
            .and_then(|section| section.get("activetab"))
            .and_then(|at| at.trim().parse::<usize>().ok())
            .map_or(tabs.len(), |at| at.min(tabs.len()));
        tabs.insert(
            at,
            TcTab {
                panel,
                position: 0,
                path: folder(path, lookup),
                caption: caption(section.and_then(|section| section.get("activecaption"))),
                locked: flag(section.and_then(|section| section.get("activelocked"))),
                active: true,
            },
        );
    }
    for (position, tab) in tabs.iter_mut().enumerate() {
        tab.position = position;
    }
    tabs
}

fn caption(value: Option<&str>) -> Option<String> {
    value.and_then(|value| checked_name(value.trim().to_owned()))
}

/// The connections `[connections]` names, in its order; a name listed twice counts once.
#[must_use]
pub fn connections(ini: &Ini) -> Vec<TcConnection> {
    let Some(list) = ini.section("connections") else {
        return Vec::new();
    };
    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();
    for (_, section) in list.entries().take(MAX_ITEMS) {
        let section = section.trim();
        let key = section.to_ascii_lowercase();
        if section.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let Some(values) = ini.section(section) else {
            continue;
        };
        let mut parts: Vec<String> = section
            .split('|')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        let name = parts.pop().unwrap_or_default();
        let (kind, host, port) = endpoint(values.get("host").unwrap_or_default());
        let kind = if parts.len() > MAX_CONNECTION_FOLDERS {
            Err(TcSkip::TooDeep)
        } else {
            kind
        };
        found.push(TcConnection {
            section: section.to_owned(),
            name,
            folders: parts,
            kind,
            host,
            port,
            user: text(values.get("username")),
            directory: text(values.get("directory")),
            password: classify(values.get("password")),
        });
    }
    found
}

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The protocol, host and port of a `host=` value.
fn endpoint(raw: &str) -> (Result<ConnectionKind, TcSkip>, String, Option<u16>) {
    let raw = raw.trim();
    let (scheme, rest) = match raw.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => (String::new(), raw),
    };
    let address = rest.split('/').next().unwrap_or_default();
    let (host, port) = match address.strip_prefix('[') {
        Some(inner) => match inner.split_once(']') {
            Some((host, after)) => (host, after.strip_prefix(':')),
            None => (address, None),
        },
        None => match address.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (host, Some(port)),
            _ => (address, None),
        },
    };
    let port = match port.map(|port| port.trim().parse::<u16>()) {
        None => Ok(None),
        Some(Ok(0) | Err(_)) => Err(TcSkip::PortInvalid),
        Some(Ok(port)) => Ok(Some(port)),
    };
    let kind = match (scheme.as_str(), port) {
        (_, Err(skip)) => Err(skip),
        ("" | "ftp", Ok(_)) => Ok(ConnectionKind::Ftp),
        ("ftps", Ok(Some(990))) => Ok(ConnectionKind::FtpsImplicit),
        ("ftps", Ok(_)) => Ok(ConnectionKind::Ftps),
        ("sftp", Ok(_)) => Ok(ConnectionKind::Ssh),
        _ => Err(TcSkip::ProtocolUnknown),
    };
    (kind, host.trim().to_owned(), port.ok().flatten())
}

#[cfg(test)]
#[path = "total_commander_tests.rs"]
mod tests;
