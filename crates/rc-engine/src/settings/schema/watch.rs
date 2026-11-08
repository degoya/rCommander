//! The panels' file watcher (RC-110-02, owner 2026-11-07), Total Commander's Configuration >
//! Refresh: `panels.watch` reads a shown folder again when the system reports a change, off by
//! default as `WatchDirs=0` is; size, date and attribute changes only with
//! `panels.watch_changes` (off, TC's bit 2); network drives and shares only with
//! `panels.watch_network` (off); `panels.watch_exclude` names drives that are never watched in
//! TC's syntax of `WatchDirsExclude` (letters, `\` for every UNC path); `panels.watch_background`
//! (on, TC's bit 8 cleared) reads while another program has the focus. Apart from the watcher,
//! `terminal.refresh_panels_on_prompt` (on) reads the shown local folders when a local or WSL
//! shell is back at its prompt after a command. None is protected.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether shown folders are read again when the system reports a change.
pub const WATCH_KEY: &str = "panels.watch";
/// Whether a change of size, date or attributes counts too, not only a new, deleted or renamed
/// entry.
pub const WATCH_CHANGES_KEY: &str = "panels.watch_changes";
/// Whether folders on network drives and shares are watched too.
pub const WATCH_NETWORK_KEY: &str = "panels.watch_network";
/// The drives never watched: letters, `\` for every UNC path.
pub const WATCH_EXCLUDE_KEY: &str = "panels.watch_exclude";
/// Whether the panels are read while another program has the focus.
pub const WATCH_BACKGROUND_KEY: &str = "panels.watch_background";
/// Whether a local shell back at its prompt reads the shown local folders again.
pub const REFRESH_ON_PROMPT_KEY: &str = "terminal.refresh_panels_on_prompt";

pub(super) const WATCH: SettingSpec = SettingSpec {
    key: WATCH_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.panels.watch",
    help: "setting.panels.watch.help",
    protected: false,
    restart: false,
};

pub(super) const WATCH_CHANGES: SettingSpec = SettingSpec {
    key: WATCH_CHANGES_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.panels.watch_changes",
    help: "setting.panels.watch_changes.help",
    protected: false,
    restart: false,
};

pub(super) const WATCH_NETWORK: SettingSpec = SettingSpec {
    key: WATCH_NETWORK_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.panels.watch_network",
    help: "setting.panels.watch_network.help",
    protected: false,
    restart: false,
};

pub(super) const WATCH_EXCLUDE: SettingSpec = SettingSpec {
    key: WATCH_EXCLUDE_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Drives { default: "" },
    label: "setting.panels.watch_exclude",
    help: "setting.panels.watch_exclude.help",
    protected: false,
    restart: false,
};

pub(super) const WATCH_BACKGROUND: SettingSpec = SettingSpec {
    key: WATCH_BACKGROUND_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.panels.watch_background",
    help: "setting.panels.watch_background.help",
    protected: false,
    restart: false,
};

pub(super) const REFRESH_ON_PROMPT: SettingSpec = SettingSpec {
    key: REFRESH_ON_PROMPT_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.terminal.refresh_panels_on_prompt",
    help: "setting.terminal.refresh_panels_on_prompt.help",
    protected: false,
    restart: false,
};

/// The drive list `text` as stored: upper-case letters and `\`, each once, in TC's order of
/// typing; spaces and commas between are dropped. `None` for anything else.
#[must_use]
pub fn drive_list(text: &str) -> Option<String> {
    let mut kept = String::new();
    for c in text.chars() {
        match c {
            ' ' | ',' | ';' => {}
            '\\' | 'A'..='Z' | 'a'..='z' => {
                let c = c.to_ascii_uppercase();
                if !kept.contains(c) {
                    kept.push(c);
                }
            }
            _ => return None,
        }
    }
    Some(kept)
}
