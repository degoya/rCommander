//! Which panels are watched, and how (RC-110-02): only the two shown panels, each while it
//! shows a plain file list on this machine's file system - no flat view (Ctrl+B, find results),
//! no archive, server or portable device, not while it shows its terminal (the other panel
//! still is). Folders on network drives and shares only with `panels.watch_network`, never a
//! drive `panels.watch_exclude` names. A WSL distribution's share is polled (`poll.rs`): Windows
//! may never report changes made inside Linux there.

use std::path::Path;

use rc_vfs::{BackendId, VfsPath, network_path, parse_wsl_unc};

use crate::{DriveKind, PanelState, PanelView};

/// How a panel's folder is watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// By the system's notices (`native.rs`).
    Native,
    /// By its modification time, once a second (`poll.rs`).
    Poll,
}

/// The watcher's settings, as read last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WatchSettings {
    /// `panels.watch`.
    pub on: bool,
    /// `panels.watch_changes`.
    pub changes: bool,
    /// `panels.watch_network`.
    pub network: bool,
    /// `panels.watch_exclude`, as stored: upper-case letters and `\`.
    pub exclude: String,
    /// `panels.watch_background`.
    pub background: bool,
}

impl Default for WatchSettings {
    /// The schema's defaults: off as in Total Commander, read in the background.
    fn default() -> Self {
        Self {
            on: false,
            changes: false,
            network: false,
            exclude: String::new(),
            background: true,
        }
    }
}

/// What the choice looks at in a panel.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shown<'a> {
    pub path: &'a VfsPath,
    /// The file list, not the terminal.
    pub files: bool,
    /// A plain folder, no flat view.
    pub plain: bool,
}

impl<'a> From<&'a PanelState> for Shown<'a> {
    fn from(state: &'a PanelState) -> Self {
        Self {
            path: &state.path,
            files: state.view == PanelView::Files,
            plain: state.flat.is_none(),
        }
    }
}

/// Whether the panel shows a plain file list of this machine: what a shell back at its prompt
/// reads again, watched or not.
pub(crate) fn local_list(shown: Shown<'_>) -> bool {
    shown.files && shown.plain && shown.path.backend == BackendId::Local
}

/// How the panel `shown` is watched, `None` for not at all; `network` when its folder lies on
/// another machine ([`on_network`]).
pub(crate) fn mode(shown: Shown<'_>, network: bool, settings: &WatchSettings) -> Option<Mode> {
    let watched = settings.on
        && local_list(shown)
        && (settings.network || !network)
        && !excluded(&shown.path.path, &settings.exclude);
    watched.then(|| {
        if parse_wsl_unc(&shown.path.path).is_some() {
            Mode::Poll
        } else {
            Mode::Native
        }
    })
}

/// Whether `path` lies on a drive `exclude` names: its letter, or `\` for a UNC path (a WSL
/// share included), as Total Commander's `WatchDirsExclude` reads.
pub(crate) fn excluded(path: &Path, exclude: &str) -> bool {
    let text = path.to_string_lossy().replace('/', "\\");
    // `\\?\C:\...` is the drive's path written long.
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => exclude
            .chars()
            .any(|named| named.eq_ignore_ascii_case(&letter)),
        (Some('\\'), Some('\\')) => exclude.contains('\\'),
        _ if path.to_string_lossy().starts_with(r"\\?\UNC") => exclude.contains('\\'),
        _ => false,
    }
}

/// Whether `path` lies on another machine: a share by its form (a WSL distribution's is this
/// machine's) or a network drive of the drive bar, `drive` being its drive's kind.
pub(crate) fn on_network(path: &VfsPath, drive: Option<DriveKind>) -> bool {
    path.backend == BackendId::Local
        && (network_path(&path.path.to_string_lossy()).is_some()
            || drive == Some(DriveKind::Network))
}

#[cfg(test)]
#[path = "choose_tests.rs"]
mod tests;
