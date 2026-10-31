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

use std::{
    collections::BTreeMap,
    sync::{PoisonError, RwLock},
};

use rc_db::connections::ConnectionThemes;
use rc_vfs::{BackendId, VfsPath};

use crate::theme::Tokens;

/// What the window knows of the connections' themes: the switch and each connection's pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// `appearance.connection_themes`.
    pub on: bool,
    /// By connection id: the light and the dark theme's id.
    pub themes: BTreeMap<i64, [Option<String>; 2]>,
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
        Self { on, themes }
    }

    /// The id of the theme `connection` draws with in the light or the `dark` mode; `None` with
    /// the switch off or no theme of its own for the mode.
    pub fn theme(&self, connection: i64, dark: bool) -> Option<&str> {
        if !self.on {
            return None;
        }
        self.themes.get(&connection)?[usize::from(dark)].as_deref()
    }
}

static ASSIGNED: RwLock<Option<Assigned>> = RwLock::new(None);

/// The connections' themes and the switch from now on.
pub fn set_assigned(assigned: Assigned) {
    *ASSIGNED.write().unwrap_or_else(PoisonError::into_inner) = Some(assigned);
}

/// The connection a location is on: a server's, over SFTP or FTP; `None` on this machine, in an
/// archive or in WSL.
pub fn connection_of(path: &VfsPath) -> Option<i64> {
    match path.backend {
        BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } => Some(connection_id),
        _ => None,
    }
}

/// The look a part of connection `connection` draws with in the light or the `dark` mode: its
/// theme's, else the window's; the window's for `None` (this machine, a local or WSL shell).
pub fn of_connection(connection: Option<i64>, dark: bool) -> &'static Tokens {
    let window = crate::theme::tokens(dark);
    let Some(connection) = connection else {
        return window;
    };
    let id = ASSIGNED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|assigned| assigned.theme(connection, dark).map(str::to_owned));
    id.and_then(|id| super::tokens_of(&id, dark))
        .unwrap_or(window)
}

/// The look a panel or a tab at `path` draws with in the light or the `dark` mode.
pub fn of_path(path: &VfsPath, dark: bool) -> &'static Tokens {
    of_connection(connection_of(path), dark)
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
