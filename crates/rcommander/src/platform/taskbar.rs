//! The taskbar's way to one more rCommander (RC-100-67). Windows starts a new instance on a
//! middle click or Shift+click on the taskbar button, and from the program's entry in the
//! button's jump list, with the command the window names (`System.AppUserModel.RelaunchCommand`,
//! honoured only beside an explicit `System.AppUserModel.ID`). A window without them leaves the
//! shell to its own guess from the process, and that guess started nothing on the owner's
//! Windows (0.15.0).
//!
//! So every window of rCommander - the main window and each dialog window, one taskbar group -
//! carries an explicit AppUserModelID, the quoted executable as its relaunch command and the
//! program's name (`windows/taskbar.rs`), and loses them again before it closes ([`leave`],
//! [`leave_all`]). The values are built here, the same on every system; only Windows has such a
//! button, elsewhere these tasks do nothing.

use std::{path::Path, sync::LazyLock};

use iced::{Task, window};

/// The AppUserModelID's company and product; the installation's mark follows.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
const APP_ID_PREFIX: &str = "degoya.rCommander.";

/// The name the jump list and a pin made from the window show for the relaunch.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
pub const DISPLAY_NAME: &str = "rCommander";

/// What a window tells the taskbar.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relaunch {
    /// `System.AppUserModel.ID`: one per installation - the executable's path, ignoring case,
    /// as a hash - so a development copy and an installed release keep apart, as they did with
    /// the ID Windows derived from the path.
    pub app_id: String,
    /// `System.AppUserModel.RelaunchCommand`: the executable, absolute and quoted, without
    /// arguments; a new instance needs nothing of the running one's command line.
    pub command: String,
}

/// This process's relaunch, worked out once; `None` when its executable has no path the shell
/// can start.
#[cfg_attr(not(windows), allow(dead_code, reason = "read by the Win32 half"))]
pub fn current() -> Option<&'static Relaunch> {
    static CURRENT: LazyLock<Option<Relaunch>> = LazyLock::new(|| {
        std::env::current_exe()
            .ok()
            .and_then(|executable| relaunch_of(&executable))
    });
    CURRENT.as_ref()
}

/// The relaunch of the program at `executable` (`std::env::current_exe()`, absolute): `None`
/// for a path that is no Unicode, is empty or holds a quote, which no command line could carry.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
pub fn relaunch_of(executable: &Path) -> Option<Relaunch> {
    let path = plain(executable.to_str()?);
    if path.is_empty() || path.contains('"') {
        return None;
    }
    Some(Relaunch {
        app_id: app_id(&path),
        command: format!("\"{path}\""),
    })
}

/// `path` without the verbatim prefix (`\\?\C:\...`, `\\?\UNC\server\...`), so the command is
/// the one a person would type and both spellings give one ID.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
fn plain(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match path.strip_prefix(r"\\?\") {
        Some(rest) if on_drive(rest) => rest.to_owned(),
        _ => path.to_owned(),
    }
}

/// Whether `path` begins with a drive letter and its colon.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
fn on_drive(path: &str) -> bool {
    matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic())
}

/// The AppUserModelID of the installation at `path`: at most 128 characters and no spaces, as
/// Windows asks, whatever the path.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
fn app_id(path: &str) -> String {
    format!(
        "{APP_ID_PREFIX}{:016x}",
        fnv1a(path.to_lowercase().as_bytes())
    )
}

/// FNV-1a over `bytes`: the same number in every build and version, which std's hasher does not
/// promise; a pin outlives an update.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Puts the window `id` into rCommander's taskbar group with the relaunch.
#[cfg(windows)]
pub fn join<T: Send + 'static>(id: window::Id) -> Task<T> {
    window::run(id, |window| {
        if let Some(handle) = crate::native_window::hwnd_of(window) {
            super::windows::taskbar::join(crate::native_window::raw_hwnd(handle));
        }
    })
    .discard()
}

/// Nothing: this system has no taskbar button that relaunches.
#[cfg(not(windows))]
pub fn join<T: Send + 'static>(_id: window::Id) -> Task<T> {
    Task::none()
}

/// Takes the relaunch off the window `id` before it closes: Windows keeps a window's
/// properties' resources unless they are removed first.
#[cfg(windows)]
pub fn leave<T: Send + 'static>(id: window::Id) -> Task<T> {
    window::run(id, |window| {
        if let Some(handle) = crate::native_window::hwnd_of(window) {
            super::windows::taskbar::leave(crate::native_window::raw_hwnd(handle));
        }
    })
    .discard()
}

/// Nothing: no window here carries a relaunch.
#[cfg(not(windows))]
pub fn leave<T: Send + 'static>(_id: window::Id) -> Task<T> {
    Task::none()
}

/// Takes the relaunch off every window that still carries it, before iced ends; on the main
/// window's thread, which owns every window of the app.
#[cfg(windows)]
pub fn leave_all<T: Send + 'static>() -> Task<T> {
    window::oldest().then(|id| match id {
        Some(id) => window::run(id, |_| super::windows::taskbar::leave_all()).discard(),
        None => Task::none(),
    })
}

/// Nothing: no window here carries a relaunch.
#[cfg(not(windows))]
pub fn leave_all<T: Send + 'static>() -> Task<T> {
    Task::none()
}

#[cfg(test)]
#[path = "taskbar_tests.rs"]
mod tests;
