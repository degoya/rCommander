//! The colour themes of the connections (RC-100-51): which theme a connection's panel and its SSH
//! terminal draw with in each mode, and whether they do (`appearance.connection_themes`).
//!
//! Only the panel takes its connection's look - its list, its path and column header, its tabs
//! and an SSH terminal of the connection; the window, the menus, the dialogs and the other panel
//! keep the window's ([`tokens`](crate::theme::tokens)), so a connection's colours never make a
//! dialog unreadable. A connection without a theme for the mode, a theme the window does not
//! know (deleted) or of the other mode, and the switch off all draw with the window's; the
//! switch off keeps the themes assigned. The looks are the ones `themes.rs` builds once
//! ([`tokens_of`](super::tokens_of)); what the window knows here is read with the themes
//! (`app/settings/themes_page.rs`) and set at the start and after every change.
//!
//! A WSL distribution is themed the same way (RC-100-56, `appearance.wsl_themes`): a panel on
//! its share (`\\wsl.localhost\<distro>\...` or `\\wsl$\<distro>\...`) and a WSL terminal of
//! it; the name compares without regard to case, and the same switch turns both off.

use std::{collections::BTreeMap, sync::RwLock};

use rc_db::connections::ConnectionThemes;
use rc_engine::settings::{WslTheme, wsl_theme};
use rc_vfs::{BackendId, VfsPath, parse_wsl_unc};

use crate::recover::RwLockOrRecover as _;
use crate::theme::Tokens;

/// What the window knows of the connections' and the WSL distributions' themes: the switch and
/// each one's pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// `appearance.connection_themes`, for the connections and the distributions alike.
    pub on: bool,
    /// By connection id: the light and the dark theme's id.
    pub themes: BTreeMap<i64, [Option<String>; 2]>,
    /// The distributions' themes, `appearance.wsl_themes` (RC-100-56).
    pub distros: Vec<WslTheme>,
}

impl Default for Assigned {
    fn default() -> Self {
        Self::of(true, Vec::new())
    }
}

impl Assigned {
    /// The switch `on` and the connections' themes as the database lists them.
    pub fn of(on: bool, rows: Vec<ConnectionThemes>) -> Self {
        let themes = rows
            .into_iter()
            .map(|row| (row.id.0, [row.light, row.dark]))
            .collect();
        Self {
            on,
            themes,
            distros: Vec::new(),
        }
    }

    /// The same, with the distributions' themes `distros`.
    #[must_use]
    pub fn with_distros(self, distros: Vec<WslTheme>) -> Self {
        Self { distros, ..self }
    }

    /// The id of the theme `connection` draws with in the light or the `dark` mode; `None` with
    /// the switch off or no theme of its own for the mode.
    pub fn theme(&self, connection: i64, dark: bool) -> Option<&str> {
        if !self.on {
            return None;
        }
        self.themes.get(&connection)?[usize::from(dark)].as_deref()
    }

    /// The id of the theme the WSL distribution `distro` draws with in the light or the `dark`
    /// mode; `None` with the switch off or no theme of its own for the mode.
    pub fn distro_theme(&self, distro: &str, dark: bool) -> Option<&str> {
        if !self.on {
            return None;
        }
        wsl_theme(&self.distros, distro)?.theme(dark)
    }
}

static ASSIGNED: RwLock<Option<Assigned>> = RwLock::new(None);

/// The connections' themes and the switch from now on.
pub fn set_assigned(assigned: Assigned) {
    *ASSIGNED.write_or_recover("connections.assigned") = Some(assigned);
}

/// The connection a location is on: a server's, over SFTP or FTP; `None` on this machine, in an
/// archive or in WSL.
pub fn connection_of(path: &VfsPath) -> Option<i64> {
    match path.backend {
        BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } => Some(connection_id),
        _ => None,
    }
}

/// The WSL distribution whose share `path` is on, either UNC form; `None` anywhere else.
pub fn distro_of(path: &VfsPath) -> Option<String> {
    match path.backend {
        BackendId::Local => parse_wsl_unc(&path.path).map(|wsl| wsl.distro),
        _ => None,
    }
}

/// The look of the theme `pick` names from what the window knows, else the window's.
fn assigned_look(dark: bool, pick: impl FnOnce(&Assigned) -> Option<&str>) -> &'static Tokens {
    let window = crate::theme::tokens(dark);
    let id = ASSIGNED
        .read_or_recover("connections.assigned")
        .as_ref()
        .and_then(|assigned| pick(assigned).map(str::to_owned));
    id.and_then(|id| super::tokens_of(&id, dark))
        .unwrap_or(window)
}

/// The look a part of connection `connection` draws with in the light or the `dark` mode: its
/// theme's, else the window's; the window's for `None` (this machine, a local shell).
pub fn of_connection(connection: Option<i64>, dark: bool) -> &'static Tokens {
    match connection {
        Some(connection) => assigned_look(dark, |assigned| assigned.theme(connection, dark)),
        None => crate::theme::tokens(dark),
    }
}

/// The look a part of the WSL distribution `distro` draws with in the light or the `dark` mode:
/// its theme's, else the window's; the window's for `None` (RC-100-56).
pub fn of_distro(distro: Option<&str>, dark: bool) -> &'static Tokens {
    match distro {
        Some(distro) => assigned_look(dark, |assigned| assigned.distro_theme(distro, dark)),
        None => crate::theme::tokens(dark),
    }
}

/// The look a panel or a tab at `path` draws with in the light or the `dark` mode: its
/// connection's on a server, its distribution's on a WSL share, else the window's.
pub fn of_path(path: &VfsPath, dark: bool) -> &'static Tokens {
    match connection_of(path) {
        Some(connection) => of_connection(Some(connection), dark),
        None => of_distro(distro_of(path).as_deref(), dark),
    }
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
