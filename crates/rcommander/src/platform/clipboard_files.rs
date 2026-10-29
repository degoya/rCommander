//! Files on the clipboard the way the Explorer puts them there (RC-080-01): Ctrl+C and Ctrl+X
//! write the panel's files as `CF_HDROP` with a "Preferred DropEffect" of copy or move, Ctrl+V
//! reads the files the Explorer copied or cut (`dropfiles.rs` makes and reads the bytes). The
//! system clipboard is Windows' (`windows/clipboard.rs`); elsewhere [`write`] and [`read`] have
//! none and the keys do nothing.

use std::path::PathBuf;

use iced::Task;

/// Whether this system has files on its clipboard at all.
pub const AVAILABLE: bool = cfg!(windows);

/// Files on the clipboard, and whether they were cut (to be moved) rather than copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

/// Puts `files` on the clipboard; whether it took them. `None` where the system has no such
/// clipboard.
#[cfg(windows)]
pub fn write(files: ClipboardFiles) -> Option<Task<bool>> {
    Some(super::windows::clipboard::write(files))
}

/// Puts `files` on the clipboard; whether it took them. `None` where the system has no such
/// clipboard.
#[cfg(not(windows))]
pub fn write(_files: ClipboardFiles) -> Option<Task<bool>> {
    None
}

/// The files on the clipboard, if it holds any. `None` where the system has no such clipboard.
#[cfg(windows)]
pub fn read() -> Option<Task<Option<ClipboardFiles>>> {
    Some(super::windows::clipboard::read())
}

/// The files on the clipboard, if it holds any. `None` where the system has no such clipboard.
#[cfg(not(windows))]
pub fn read() -> Option<Task<Option<ClipboardFiles>>> {
    None
}
