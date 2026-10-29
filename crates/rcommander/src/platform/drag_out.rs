//! Dragging files out of rCommander into other programs (RC-080-01): the Explorer, a mail, a
//! browser's upload. Windows has it (`windows/drag_out.rs`: the shell's data object of the files
//! and its own drag loop, drag image included); elsewhere [`start`] has nothing and a drag that
//! leaves the window simply goes on inside it (ADR 0001). [`pointer`] tells where a drop from
//! another program landed, which the window's file-drop events do not say.

use std::path::PathBuf;

use iced::Task;

/// Whether this system lets files be dragged out of the window.
pub const AVAILABLE: bool = cfg!(windows);

/// How a drag out of the window ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "made by the Windows drag only")
)]
pub enum DragOutcome {
    /// The files were dropped somewhere, or the drag was given up; they may have moved.
    Ended,
    /// The mouse button was up already, so no drag started: it would have dropped at once,
    /// wherever the pointer stood.
    NotHeld,
    /// The system could not make the files draggable.
    Failed,
}

/// The pointer over the window when a drop from another program landed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "made by the Windows pointer query only")
)]
pub struct Pointer {
    /// Where across the window's width, from 0 (left edge) to 1 (right edge).
    pub across: f32,
    /// Whether Shift is held: a move instead of a copy.
    pub shift: bool,
}

/// Drags `paths` (absolute, local) out of the window while the mouse button stays down; `None`
/// where the system cannot.
#[cfg(windows)]
pub fn start(paths: Vec<PathBuf>) -> Option<Task<DragOutcome>> {
    Some(super::windows::drag_out::task(paths))
}

/// Drags `paths` (absolute, local) out of the window while the mouse button stays down; `None`
/// where the system cannot.
#[cfg(not(windows))]
pub fn start(_paths: Vec<PathBuf>) -> Option<Task<DragOutcome>> {
    None
}

/// The pointer over the window and the Shift key, now; `None` where the system does not say.
#[cfg(windows)]
pub fn pointer() -> Task<Option<Pointer>> {
    super::windows::drag_out::pointer()
}

/// The pointer over the window and the Shift key, now; `None` where the system does not say.
#[cfg(not(windows))]
pub fn pointer() -> Task<Option<Pointer>> {
    Task::done(None)
}
