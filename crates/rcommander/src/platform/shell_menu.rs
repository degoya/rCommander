//! The Explorer's context menu for entries of a local folder (RC-080-02), where the system has
//! one: Windows (`windows/context_menu.rs`). Elsewhere [`show`] has none and the window draws
//! rCommander's own menu (`app/context_menu.rs`).

use std::path::PathBuf;

use iced::Task;

/// Whether this system has a shell menu at all.
pub const AVAILABLE: bool = cfg!(windows);

/// What the shell menu is asked for.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows shell menu only")
)]
pub struct ShellRequest {
    /// The folder the entries are in, or whose own menu ("New", "Paste") is wanted.
    pub folder: PathBuf,
    /// The entries, by name, all in `folder`; none for the folder's own menu.
    pub names: Vec<String>,
    /// The labels of rCommander's entries above the shell's, in the order of the window's menu.
    pub own: Vec<String>,
    /// Where the menu opens.
    pub anchor: Anchor,
}

/// Where the shell menu opens.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows shell menu only")
)]
pub enum Anchor {
    /// At the mouse pointer: a right click.
    Pointer,
    /// At this point of the window, in logical pixels: the cursor's row, for the keyboard.
    Window { x: f32, y: f32 },
}

/// How the shell menu ended.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "made by the Windows shell menu only")
)]
pub enum ShellOutcome {
    /// It closed without a choice.
    Dismissed,
    /// One of rCommander's own entries, by its index in [`ShellRequest::own`].
    Run(usize),
    /// The shell's "Rename": rCommander renames in place instead, as the Explorer's view would.
    Rename,
    /// A shell entry ran; files may have changed.
    Invoked,
    /// No menu could be made (a path the shell does not parse); rCommander's own opens instead.
    Failed,
}

/// The shell menu for `request` on the app's window, modal on the window's thread; `None` where
/// the system has none.
#[cfg(windows)]
pub fn show(request: ShellRequest) -> Option<Task<ShellOutcome>> {
    Some(super::windows::context_menu::task(request))
}

/// The shell menu for `request` on the app's window, modal on the window's thread; `None` where
/// the system has none.
#[cfg(not(windows))]
pub fn show(_request: ShellRequest) -> Option<Task<ShellOutcome>> {
    None
}
