//! The system's own dialog for choosing a file or a folder (RC-010-18), behind the "Browse..."
//! button beside a path field (`app/browse.rs`): Windows' Common Item Dialog
//! (`windows/file_dialog.rs`). Elsewhere there is none for now and the button is not drawn.
//!
//! The dialog only fills the field: what it answers goes the way a typed path goes, through the
//! same checks. Only the window opens it - no command, no MCP tool.

use std::path::{Path, PathBuf};

use iced::Task;

/// Whether this system has a file dialog at all.
pub const AVAILABLE: bool = cfg!(windows);

/// What the dialog chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An existing file to read.
    Open,
    /// A file to write; it may not exist yet.
    Save,
    /// An existing folder.
    Folder,
}

/// A file type the dialog offers: its text in the list and its patterns (`*.exe;*.cmd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows dialog only")
)]
pub struct Filter {
    pub label: &'static str,
    pub spec: &'static str,
}

/// Where the dialog opens when the field names no folder that exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// Where the system's dialog opened last.
    Dialog,
    /// The `.ssh` folder in the home folder, where private keys and `ssh_config` live.
    SshFolder,
}

impl Fallback {
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "read by the Windows dialog only")
    )]
    pub fn folder(self) -> Option<PathBuf> {
        match self {
            Self::Dialog => None,
            Self::SshFolder => {
                directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".ssh"))
            }
        }
    }
}

/// What the dialog is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows dialog only")
)]
pub struct Request {
    pub kind: Kind,
    /// The file types offered, the first one chosen; none for a folder.
    pub filters: Vec<Filter>,
    /// The field's text as typed.
    pub current: String,
    pub fallback: Fallback,
}

/// Where the dialog opens and the name it suggests.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "made for the Windows dialog only")
)]
pub struct Start {
    pub folder: Option<PathBuf>,
    pub name: Option<String>,
}

/// Where the dialog for `kind` opens on a field holding `current`: the folder it names, else the
/// nearest folder above it that exists, with the rest's last part as the suggested file name;
/// `fallback` when the field is empty, relative or names nothing that exists. `is_dir` asks the
/// file system - off the window's thread, as it may be a slow network drive.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "called by the Windows dialog only")
)]
pub fn start(
    kind: Kind,
    current: &str,
    fallback: Option<PathBuf>,
    is_dir: impl Fn(&Path) -> bool,
) -> Start {
    let fallback = fallback.filter(|folder| is_dir(folder));
    let current = Path::new(current.trim());
    if !current.is_absolute() {
        return Start {
            folder: fallback,
            name: None,
        };
    }
    if is_dir(current) {
        return Start {
            folder: Some(current.to_path_buf()),
            name: None,
        };
    }
    let name = match kind {
        Kind::Folder => None,
        Kind::Open | Kind::Save => current
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
    };
    let folder = current
        .ancestors()
        .skip(1)
        .find(|folder| is_dir(folder))
        .map(Path::to_path_buf)
        .or(fallback);
    Start { folder, name }
}

/// The dialog for `request`, modal to the app's window but on a thread of its own, so the window
/// keeps drawing; the path chosen, `None` when it was cancelled.
#[cfg(windows)]
pub fn pick(request: Request) -> Task<Option<PathBuf>> {
    super::windows::file_dialog::task(request)
}

/// The dialog for `request`; none on this system, so nothing is chosen.
#[cfg(not(windows))]
pub fn pick(_request: Request) -> Task<Option<PathBuf>> {
    Task::done(None)
}

#[cfg(test)]
#[path = "file_dialog_tests.rs"]
mod tests;
