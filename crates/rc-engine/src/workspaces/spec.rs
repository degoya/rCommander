//! A workspace's model (RC-100-68): what each panel shows ([`PanelSpec`]), as versioned JSON
//! ([`WorkspaceSpec`]), and the checks every spec passes on its way in - from the window, from
//! MCP, and back from the database.
//!
//! The limits: 1 to [`MAX_TABS`] tabs a panel, [`MAX_SPEC_BYTES`] of JSON, a name of 1 to
//! [`MAX_NAME_CHARS`] characters without control characters; a tab on this machine (a WSL share
//! included) or on a server of the connection tree, never inside an archive; every connection
//! the spec names must exist. Another [`SPEC_VERSION`] is refused before its fields are read.

use std::collections::BTreeSet;

use rc_vfs::{BackendId, VfsPath};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::WorkspaceError;
use crate::{ConnectionId, ListMode, PanelId, PanelView, SortOrder};

/// A workspace's id in the database.
pub type WorkspaceId = i64;

/// The version of the spec this build writes and reads.
pub const SPEC_VERSION: u32 = 1;
/// The most tabs a panel of a workspace has.
pub const MAX_TABS: usize = 32;
/// The longest spec, as JSON.
pub const MAX_SPEC_BYTES: usize = 65536;
/// The longest name, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// The longest folder, start folder, shell or distribution name, in bytes.
pub const MAX_TEXT: usize = 4096;

/// The divider's bounds, as `panels.split_permille` has them.
const SPLIT: std::ops::RangeInclusive<u16> = 100..=900;

/// What both panels show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceSpec {
    /// [`SPEC_VERSION`]; another is refused (`workspace_version`).
    pub version: u32,
    /// The panel keyboard input goes to after the workspace is applied.
    pub active: PanelId,
    /// The left panel's share of the width, 100 to 900 per mille; `None` leaves the divider.
    #[serde(default)]
    pub split_permille: Option<u16>,
    pub left: PanelSpec,
    pub right: PanelSpec,
}

/// What one panel shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelSpec {
    /// Its tabs, left to right: 1 to [`MAX_TABS`].
    pub tabs: Vec<TabSpec>,
    /// The tab it shows, an index into `tabs`.
    pub active_tab: usize,
    /// The file list or the terminal.
    #[serde(default)]
    pub view: PanelView,
    /// The terminal it shows; required for the terminal view.
    #[serde(default)]
    pub terminal: Option<TermSpec>,
}

/// One tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TabSpec {
    /// Its folder: on this machine (a WSL share too) or on a server of the connection tree.
    pub location: VfsPath,
    /// Its order; the panel's when `None`.
    #[serde(default)]
    pub sort: Option<SortOrder>,
    /// Full or brief; the panel's when `None`.
    #[serde(default)]
    pub mode: Option<ListMode>,
    /// A locked tab keeps its folder (RC-010-03).
    #[serde(default)]
    pub locked: bool,
}

/// The terminal a panel shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TermSpec {
    /// A shell of this machine: the shell id `terminal.list` names, the default one when `None`.
    Local {
        #[serde(default)]
        shell: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    /// A shell of a WSL distribution.
    Wsl {
        distro: String,
        #[serde(default)]
        cwd: Option<String>,
    },
    /// A shell on the server of an SSH connection.
    Ssh {
        connection_id: ConnectionId,
        #[serde(default)]
        cwd: Option<String>,
    },
}

/// A workspace as `workspaces.list` and `workspaces.get` answer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub spec: WorkspaceSpec,
    /// The theme id of the light mode while the workspace is active; `None` leaves the window's.
    pub theme_light: Option<String>,
    /// The theme id of the dark mode while the workspace is active.
    pub theme_dark: Option<String>,
    /// It is the workspace applied last in this window.
    pub active: bool,
}

/// What applying a workspace does to the tabs a panel has (`workspaces.apply_mode`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    /// The workspace's tabs take the place of the panel's; its locked tabs stay, before them.
    ReplaceKeepLocked,
    /// The workspace's tabs take the place of all of the panel's: the panel shows exactly the
    /// workspace's tabs (the default, owner 2026-11-02, RC-100-72).
    #[default]
    ReplaceAll,
    /// The workspace's tabs come after the panel's.
    Add,
}

impl ApplyMode {
    /// The mode a setting's value names; the default for anything else.
    #[must_use]
    pub fn of_setting(value: &str) -> Self {
        match value {
            "replace_keep_locked" => Self::ReplaceKeepLocked,
            "add" => Self::Add,
            _ => Self::ReplaceAll,
        }
    }
}

/// What the `workspaces.*` commands answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum WorkspacesOutcome {
    /// `workspaces.list`: every workspace in the person's order.
    List { workspaces: Vec<Workspace> },
    /// `workspaces.get`; boxed, it is far larger than the other answers.
    Workspace(Box<Workspace>),
    /// `workspaces.create` and `workspaces.capture`: the workspace saved.
    Saved { id: WorkspaceId },
    /// `workspaces.update`, `.rename`, `.move`, `.delete`.
    Done {},
    /// `workspaces.apply`: the parts left out, each `<error code>:<what>` - the code the
    /// window's `error.<code>` text explains, then the folder, connection or terminal.
    Applied { skipped: Vec<String> },
}

/// What a check of a spec needs to know beyond the spec: the connections that exist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecContext {
    pub connections: BTreeSet<ConnectionId>,
}

impl WorkspaceSpec {
    /// The spec of `panel`.
    #[must_use]
    pub fn panel(&self, panel: PanelId) -> &PanelSpec {
        match panel {
            PanelId::Left => &self.left,
            PanelId::Right => &self.right,
        }
    }

    /// Every connection the spec names: its tabs' servers and its SSH terminals.
    #[must_use]
    pub fn connections(&self) -> BTreeSet<ConnectionId> {
        let mut found = BTreeSet::new();
        for panel in [&self.left, &self.right] {
            for tab in &panel.tabs {
                if let Some(id) = connection_of(&tab.location) {
                    found.insert(id);
                }
            }
            if let Some(TermSpec::Ssh { connection_id, .. }) = &panel.terminal {
                found.insert(*connection_id);
            }
        }
        found
    }

    /// Refuses a spec that breaks a limit or names what `context` does not know.
    ///
    /// # Errors
    ///
    /// `workspace_version`, `workspace_tabs`, `workspace_terminal`, `workspace_location`,
    /// `workspace_connection`, `workspace_invalid` (the divider), `workspace_too_large`.
    pub fn check(&self, context: &SpecContext) -> Result<(), WorkspaceError> {
        if self.version != SPEC_VERSION {
            return Err(WorkspaceError::Version {
                found: u64::from(self.version),
            });
        }
        if let Some(split) = self.split_permille
            && !SPLIT.contains(&split)
        {
            return Err(WorkspaceError::Invalid {
                reason: "the divider lies between 100 and 900 per mille".into(),
            });
        }
        self.left.check(context)?;
        self.right.check(context)?;
        self.to_json().map(drop)
    }

    /// The spec as the database keeps it.
    ///
    /// # Errors
    ///
    /// `workspace_too_large` beyond [`MAX_SPEC_BYTES`].
    pub fn to_json(&self) -> Result<String, WorkspaceError> {
        let text = serde_json::to_string(self).map_err(|error| WorkspaceError::Invalid {
            reason: error.to_string(),
        })?;
        too_large(text.len())?;
        Ok(text)
    }

    /// A spec read from JSON text: its size, then its version, then its fields. It is not
    /// [`checked`](Self::check) yet.
    ///
    /// # Errors
    ///
    /// `workspace_too_large`, `workspace_version`, `workspace_invalid`.
    pub fn parse(text: &str) -> Result<Self, WorkspaceError> {
        too_large(text.len())?;
        let value: Value = serde_json::from_str(text).map_err(|error| WorkspaceError::Invalid {
            reason: error.to_string(),
        })?;
        Self::from_value(value)
    }

    /// A spec from a JSON value, as [`WorkspaceSpec::parse`] reads one.
    ///
    /// # Errors
    ///
    /// `workspace_version`, `workspace_invalid`.
    pub fn from_value(value: Value) -> Result<Self, WorkspaceError> {
        let version = value.get("version").and_then(Value::as_u64);
        if version != Some(u64::from(SPEC_VERSION)) {
            return Err(WorkspaceError::Version {
                found: version.unwrap_or(0),
            });
        }
        serde_json::from_value(value).map_err(|error| WorkspaceError::Invalid {
            reason: error.to_string(),
        })
    }
}

impl PanelSpec {
    fn check(&self, context: &SpecContext) -> Result<(), WorkspaceError> {
        if self.tabs.is_empty() || self.tabs.len() > MAX_TABS {
            return Err(WorkspaceError::Tabs {
                reason: format!("a panel has 1 to {MAX_TABS} tabs, not {}", self.tabs.len()),
            });
        }
        if self.active_tab >= self.tabs.len() {
            return Err(WorkspaceError::Tabs {
                reason: format!("the panel has no tab {}", self.active_tab),
            });
        }
        for tab in &self.tabs {
            check_location(&tab.location, context)?;
        }
        if self.view == PanelView::Terminal && self.terminal.is_none() {
            return Err(WorkspaceError::Terminal {
                reason: "the terminal view needs a terminal".into(),
            });
        }
        match &self.terminal {
            Some(terminal) => terminal.check(context),
            None => Ok(()),
        }
    }
}

impl TermSpec {
    fn check(&self, context: &SpecContext) -> Result<(), WorkspaceError> {
        let cwd = match self {
            Self::Local { shell, cwd } => {
                if shell.as_deref().is_some_and(|shell| !is_plain(shell)) {
                    return Err(terminal("the shell is not one of terminal.list's ids"));
                }
                cwd
            }
            Self::Wsl { distro, cwd } => {
                if !is_plain(distro) || distro.contains(['/', '\\']) {
                    return Err(terminal("the WSL distribution's name does not fit"));
                }
                cwd
            }
            Self::Ssh { connection_id, cwd } => {
                if !context.connections.contains(connection_id) {
                    return Err(WorkspaceError::Connection { id: *connection_id });
                }
                cwd
            }
        };
        if cwd.as_deref().is_some_and(|cwd| !is_text(cwd)) {
            return Err(terminal("the start folder does not fit"));
        }
        Ok(())
    }

    /// The terminal in words for a note: its kind and what it runs on.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Local { shell, .. } => format!("local:{}", shell.as_deref().unwrap_or("")),
            Self::Wsl { distro, .. } => format!("wsl:{distro}"),
            Self::Ssh { connection_id, .. } => format!("ssh:{}", connection_id.0),
        }
    }
}

fn terminal(reason: &str) -> WorkspaceError {
    WorkspaceError::Terminal {
        reason: reason.to_owned(),
    }
}

/// The connection a location is on: a server's, over SFTP or FTP.
pub(super) fn connection_of(location: &VfsPath) -> Option<ConnectionId> {
    match location.backend {
        BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } => {
            Some(ConnectionId(connection_id))
        }
        _ => None,
    }
}

fn check_location(location: &VfsPath, context: &SpecContext) -> Result<(), WorkspaceError> {
    let refused = |reason: &str| WorkspaceError::Location {
        reason: reason.to_owned(),
    };
    let text = location.path.to_string_lossy();
    if text.is_empty() || text.len() > MAX_TEXT || text.contains('\0') {
        return Err(refused("the folder does not fit"));
    }
    match &location.backend {
        BackendId::Local if location.path.is_absolute() => Ok(()),
        BackendId::Local => Err(refused("a folder of this machine is an absolute path")),
        BackendId::Sftp { .. } | BackendId::Ftp { .. } => match connection_of(location) {
            Some(id) if context.connections.contains(&id) => Ok(()),
            Some(id) => Err(WorkspaceError::Connection { id }),
            None => Err(refused("no server")),
        },
        _ => Err(refused(
            "a tab of a workspace is on this machine or on a server, never inside an archive",
        )),
    }
}

/// A name as a workspace keeps it: trimmed, 1 to [`MAX_NAME_CHARS`] characters, no control
/// characters.
///
/// # Errors
///
/// `workspace_name`.
pub fn clean_name(name: &str) -> Result<String, WorkspaceError> {
    let refused = |reason: &str| WorkspaceError::Name {
        reason: reason.to_owned(),
    };
    if name.chars().any(char::is_control) {
        return Err(refused("a name has no control characters"));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(refused("a workspace needs a name"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(refused("the name is too long"));
    }
    Ok(name.to_owned())
}

/// Whether `text` fits a folder or start folder: not empty, at most [`MAX_TEXT`] bytes, no
/// control characters.
pub(super) fn is_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_TEXT && !text.chars().any(char::is_control)
}

/// Whether `name` fits a shell id or a distribution: 1 to 64 characters, no control
/// characters, no surrounding blanks.
pub(super) fn is_plain(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

fn too_large(bytes: usize) -> Result<(), WorkspaceError> {
    if bytes > MAX_SPEC_BYTES {
        return Err(WorkspaceError::TooLarge {
            bytes,
            max: MAX_SPEC_BYTES,
        });
    }
    Ok(())
}

/// A small valid spec on this machine, for tests.
#[cfg(test)]
pub(crate) fn sample() -> WorkspaceSpec {
    let tab = |path: &str| TabSpec {
        location: VfsPath::local(std::env::temp_dir().join(path)),
        sort: None,
        mode: None,
        locked: false,
    };
    WorkspaceSpec {
        version: SPEC_VERSION,
        active: PanelId::Right,
        split_permille: Some(400),
        left: PanelSpec {
            tabs: vec![tab("a"), tab("b")],
            active_tab: 1,
            view: PanelView::Files,
            terminal: None,
        },
        right: PanelSpec {
            tabs: vec![tab("c")],
            active_tab: 0,
            view: PanelView::Files,
            terminal: None,
        },
    }
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
