//! A panel of the form (RC-100-68): its tabs - each a place of the drive bar with a path in it,
//! locked or not -, the active tab, the view and the terminal. The places are what the drive bar
//! lists ([`places`]): the drives, the WSL distributions and the servers, each with the root a
//! panel goes to; a tab's place is the drive its location lies on (`rc_engine::drive_of`).
//!
//! A panel keeps at least one tab: the last one is not removed. The active tab stays the same
//! tab when the tabs move; removed, the one before it is active.

use std::{fmt, path::PathBuf};

use rc_engine::{ConnectionId, Drive, DriveKind, PanelSpec, PanelView, TermSpec, drive_of};
use rc_vfs::{BackendId, VfsPath};

/// A change of one panel of the form.
#[derive(Debug, Clone)]
pub enum PanelMsg {
    /// Tab `n` goes to a place's root.
    Place(usize, VfsPath),
    /// Tab `n`'s path, as typed; its place stays.
    Path(usize, String),
    /// Tab `n` takes the folder the panel shows now (the window fills it in).
    FromPanel(usize),
    /// Tab `n` at this location.
    Location(usize, VfsPath),
    /// A new tab at the folder the panel shows now (the window fills it in).
    Add,
    AddTab(VfsPath),
    Up(usize),
    Down(usize),
    Remove(usize),
    Locked(usize, bool),
    ActiveTab(usize),
    View(PanelView),
    /// The terminal as a whole: its kind, shell, distribution, server or start folder changed.
    Terminal(TermSpec),
}

/// A place of the drive bar a tab can lie on.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The drive bar's id: `C:`, a mount point, a distribution's share, `server:<id>`.
    pub id: String,
    pub label: String,
    pub root: VfsPath,
}

impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// The kinds of terminal a panel can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermKind {
    Local,
    Wsl,
    Ssh,
}

/// Every place of `drives` that a panel can go to: each drive, distribution and server with its
/// root; an FTP server, which has none before it is opened, at `/`.
pub fn places(drives: &[Drive]) -> Vec<Place> {
    drives
        .iter()
        .filter_map(|drive| {
            let root = drive.root.clone().or_else(|| server_root(drive))?;
            Some(Place {
                id: drive.id.clone(),
                label: drive.label.clone(),
                root,
            })
        })
        .collect()
}

/// An FTP server's root: `/` on the connection the drive's id names.
fn server_root(drive: &Drive) -> Option<VfsPath> {
    if drive.kind != DriveKind::Server {
        return None;
    }
    let connection_id = drive.id.strip_prefix("server:")?.parse::<i64>().ok()?;
    Some(VfsPath {
        backend: BackendId::Ftp { connection_id },
        path: PathBuf::from("/"),
    })
}

/// The place of `places` that `location` lies on, by the drives of the drive bar.
pub fn place_of<'p>(
    places: &'p [Place],
    drives: &[Drive],
    location: &VfsPath,
) -> Option<&'p Place> {
    let drive = drive_of(drives, location)?;
    places.iter().find(|place| place.id == drive.id)
}

/// The WSL distributions of `drives`, by name.
pub fn distros(drives: &[Drive]) -> Vec<String> {
    drives
        .iter()
        .filter(|drive| drive.kind == DriveKind::Wsl)
        .map(|drive| drive.label.clone())
        .collect()
}

/// The SSH servers of `drives` - those a panel reaches over SFTP - with their names.
pub fn ssh_servers(drives: &[Drive]) -> Vec<(ConnectionId, String)> {
    drives
        .iter()
        .filter_map(
            |drive| match drive.root.as_ref().map(|root| &root.backend) {
                Some(BackendId::Sftp { connection_id }) => {
                    Some((ConnectionId(*connection_id), drive.label.clone()))
                }
                _ => None,
            },
        )
        .collect()
}

/// The kind of `terminal`.
pub fn kind_of(terminal: &TermSpec) -> TermKind {
    match terminal {
        TermSpec::Local { .. } => TermKind::Local,
        TermSpec::Wsl { .. } => TermKind::Wsl,
        TermSpec::Ssh { .. } => TermKind::Ssh,
    }
}

/// The start folder of `terminal`, empty for its default.
pub fn cwd_of(terminal: &TermSpec) -> &str {
    match terminal {
        TermSpec::Local { cwd, .. } | TermSpec::Wsl { cwd, .. } | TermSpec::Ssh { cwd, .. } => {
            cwd.as_deref().unwrap_or_default()
        }
    }
}

/// `terminal` with the start folder `typed`; an empty one is the default.
pub fn with_cwd(terminal: &TermSpec, typed: String) -> TermSpec {
    let typed = Some(typed).filter(|typed| !typed.trim().is_empty());
    let mut changed = terminal.clone();
    match &mut changed {
        TermSpec::Local { cwd, .. } | TermSpec::Wsl { cwd, .. } | TermSpec::Ssh { cwd, .. } => {
            *cwd = typed;
        }
    }
    changed
}

/// A local `terminal` with the shell `typed`; an empty one is the default shell. Another kind
/// stays as it is.
pub fn with_shell(terminal: &TermSpec, typed: String) -> TermSpec {
    match terminal {
        TermSpec::Local { cwd, .. } => TermSpec::Local {
            shell: Some(typed).filter(|typed| !typed.trim().is_empty()),
            cwd: cwd.clone(),
        },
        other => other.clone(),
    }
}

/// A terminal of `kind` with the start folder of `terminal`: a local one with the default shell,
/// a WSL one of `distro`, an SSH one on `server`; `None` without the distribution or server it
/// needs.
pub fn switched(
    terminal: Option<&TermSpec>,
    kind: TermKind,
    distro: Option<&str>,
    server: Option<ConnectionId>,
) -> Option<TermSpec> {
    let cwd = terminal
        .map(cwd_of)
        .filter(|cwd| !cwd.is_empty())
        .map(str::to_owned);
    Some(match kind {
        TermKind::Local => TermSpec::Local { shell: None, cwd },
        TermKind::Wsl => TermSpec::Wsl {
            distro: distro?.to_owned(),
            cwd,
        },
        TermKind::Ssh => TermSpec::Ssh {
            connection_id: server?,
            cwd,
        },
    })
}

/// `message` applied to `panel`; the messages the window fills in first change nothing here.
pub fn apply(panel: &mut PanelSpec, message: PanelMsg) {
    match message {
        PanelMsg::Place(tab, location) | PanelMsg::Location(tab, location) => {
            if let Some(tab) = panel.tabs.get_mut(tab) {
                tab.location = location;
            }
        }
        PanelMsg::Path(tab, typed) => {
            if let Some(tab) = panel.tabs.get_mut(tab) {
                tab.location.path = PathBuf::from(typed);
            }
        }
        PanelMsg::AddTab(location) => {
            let mut tab = panel
                .tabs
                .get(panel.active_tab)
                .cloned()
                .unwrap_or_else(|| rc_engine::TabSpec {
                    location: location.clone(),
                    sort: None,
                    mode: None,
                    locked: false,
                    title: None,
                });
            tab.location = location;
            tab.locked = false;
            panel.tabs.push(tab);
        }
        PanelMsg::Up(tab) => swap(panel, tab, tab.checked_sub(1)),
        PanelMsg::Down(tab) => swap(panel, tab, Some(tab + 1)),
        PanelMsg::Remove(tab) => remove(panel, tab),
        PanelMsg::Locked(tab, locked) => {
            if let Some(tab) = panel.tabs.get_mut(tab) {
                tab.locked = locked;
            }
        }
        PanelMsg::ActiveTab(tab) => {
            if tab < panel.tabs.len() {
                panel.active_tab = tab;
            }
        }
        PanelMsg::View(view) => {
            panel.view = view;
            // A terminal panel starts one: a local shell until another is chosen.
            if view == PanelView::Terminal && panel.terminal.is_none() {
                panel.terminal = Some(TermSpec::Local {
                    shell: None,
                    cwd: None,
                });
            }
        }
        PanelMsg::Terminal(terminal) => panel.terminal = Some(terminal),
        PanelMsg::FromPanel(_) | PanelMsg::Add => {}
    }
}

/// Tabs `at` and `other` change places; the active tab goes with its tab.
fn swap(panel: &mut PanelSpec, at: usize, other: Option<usize>) {
    let Some(other) = other.filter(|other| *other < panel.tabs.len()) else {
        return;
    };
    if at >= panel.tabs.len() {
        return;
    }
    panel.tabs.swap(at, other);
    if panel.active_tab == at {
        panel.active_tab = other;
    } else if panel.active_tab == other {
        panel.active_tab = at;
    }
}

/// Tab `at` goes, unless it is the last; the active tab stays on its tab, the one before for
/// the removed one.
fn remove(panel: &mut PanelSpec, at: usize) {
    if panel.tabs.len() < 2 || at >= panel.tabs.len() {
        return;
    }
    panel.tabs.remove(at);
    if panel.active_tab > at || (panel.active_tab == at && at > 0) {
        panel.active_tab -= 1;
    }
    panel.active_tab = panel.active_tab.min(panel.tabs.len() - 1);
}

#[cfg(test)]
#[path = "panel_edit_tests.rs"]
mod tests;
