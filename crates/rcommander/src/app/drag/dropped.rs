//! Paths another program hands over - dropped onto the window, pasted with Ctrl+V - and the
//! operation that copies them into a panel. Without a window, so it is tested as it is.
//!
//! The paths are hostile input: they come from any program that can start a drag or write the
//! clipboard. Only plain absolute paths are taken - no `.` or `..` parts, no device or `\\?\`
//! namespace other than a drive or a share, no alternate data stream, no NUL, no path longer than
//! Windows has - at most [`MAX_PATHS`] of them, each once. They are never opened or run here;
//! they become the sources of `ops.copy` (or `ops.move`), whose job reads them like any other.

use std::{
    collections::HashSet,
    path::{Component, MAIN_SEPARATOR, Path, PathBuf},
};

use rc_engine::{Command, JobOptions, PanelId, TransferRequest, VfsPath};
use rc_vfs::BackendId;

/// The most paths taken from one drop or paste.
pub const MAX_PATHS: usize = 10_000;
/// The longest path taken, in characters: the longest path Windows has.
const MAX_PATH_CHARS: usize = 32_767;

/// The paths taken from `paths`, in their order, and how many were refused. `windows` applies
/// Windows' rules for prefixes and streams (the window passes `cfg!(windows)`).
pub fn accept(paths: Vec<PathBuf>, windows: bool) -> (Vec<PathBuf>, usize) {
    let offered = paths.len();
    let mut taken: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for path in paths {
        if taken.len() == MAX_PATHS {
            break;
        }
        if plain(&path, windows) && seen.insert(path.clone()) {
            taken.push(path);
        }
    }
    let refused = offered - taken.len();
    (taken, refused)
}

/// Whether `path` is a plain absolute path of a file system.
fn plain(path: &Path, windows: bool) -> bool {
    let Some(text) = path.to_str() else {
        // Not Unicode: no path the Explorer hands out, and nothing a job could name in a dialog.
        return false;
    };
    if text.is_empty() || text.chars().count() > MAX_PATH_CHARS || text.contains('\0') {
        return false;
    }
    // `components` drops a `.` inside a path, so the text is read part by part as well.
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        || text
            .split(['/', '\\'])
            .any(|part| part == "." || part == "..")
    {
        return false;
    }
    !windows || plain_on_windows(text)
}

/// Windows' rules: a drive (`C:\`) or a share (`\\server\share`), also behind `\\?\`; no device
/// namespace (`\\.\`, `\\?\GLOBALROOT`) and no `:` after the drive (an alternate data stream).
fn plain_on_windows(text: &str) -> bool {
    let rest = match text.strip_prefix(r"\\?\") {
        Some(verbatim) => match verbatim.strip_prefix(r"UNC\") {
            Some(share) => return share_path(share),
            None => verbatim,
        },
        None if text.starts_with(r"\\.\") || text.starts_with(r"//./") => return false,
        None => match text.strip_prefix(r"\\").or_else(|| text.strip_prefix("//")) {
            Some(share) => return share_path(share),
            None => text,
        },
    };
    let drive = rc_vfs::drive_letter(rest).is_some()
        && matches!(rest.as_bytes().get(2), Some(b'\\' | b'/'));
    drive && !rest[2..].contains(':')
}

/// `server\share\...` after the leading `\\`: a server and a share, and no stream.
fn share_path(text: &str) -> bool {
    let mut parts = text.split(['\\', '/']);
    let server = parts.next().unwrap_or_default();
    let share = parts.next().unwrap_or_default();
    !server.is_empty() && !share.is_empty() && !text.contains(':')
}

/// Why dropped or pasted files go nowhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Nothing is left to copy: no path was taken, or every one is in the panel's folder already.
    Nothing,
    /// Neither panel is local, so no command can name local sources and the target together.
    NoLocalPanel,
}

/// The operation that copies (or, `moving`, moves) local `sources` into the folder of panel
/// `target`, `target_dir`, whose other panel shows `other_dir`. Sources already in that folder
/// are left out: dropping a file where it lies does nothing, as in the Explorer.
///
/// `ops.copy` names its target relative to the other panel and its sources relative to its own
/// panel, each on that panel's backend: with a local other panel the sources go from it, the
/// target is `target`'s folder whatever its backend (an upload to a remote folder); with a
/// local `target` the folder's own `ops.copy_to` takes both.
pub fn transfer(
    target: PanelId,
    target_dir: &VfsPath,
    other_dir: &VfsPath,
    sources: Vec<PathBuf>,
    moving: bool,
) -> Result<Command, Refusal> {
    let local_target = target_dir.backend == BackendId::Local;
    let sources: Vec<String> = sources
        .into_iter()
        .filter(|source| !(local_target && source.parent() == Some(target_dir.path.as_path())))
        .map(|source| source.display().to_string())
        .collect();
    if sources.is_empty() {
        return Err(Refusal::Nothing);
    }
    let options = JobOptions::default();
    if other_dir.backend == BackendId::Local {
        let request = TransferRequest {
            panel: Some(target.other()),
            sources,
            target: None,
            options,
        };
        return Ok(if moving {
            Command::OpsMove(request)
        } else {
            Command::OpsCopy(request)
        });
    }
    if !local_target {
        return Err(Refusal::NoLocalPanel);
    }
    let panel = Some(target);
    let into = into_folder(&target_dir.path);
    Ok(if moving {
        Command::OpsMoveTo {
            panel,
            sources,
            target: into,
            options,
            queue: None,
        }
    } else {
        Command::OpsCopyTo {
            panel,
            sources,
            target: into,
            options,
            queue: None,
        }
    })
}

/// The panel a drop lands in: the maximized one wherever it lands, else the side of the divider
/// (at `left_share` per mille) that `across` (0 at the window's left edge, 1 at its right)
/// falls on; `None` without a position.
pub fn target_panel(
    across: Option<f32>,
    left_share: u16,
    maximized: Option<PanelId>,
) -> Option<PanelId> {
    if maximized.is_some() {
        return maximized;
    }
    let across = across?;
    Some(if across < f32::from(left_share) / 1000.0 {
        PanelId::Left
    } else {
        PanelId::Right
    })
}

/// `path` as a target to copy into: with a separator at its end, as the F5 dialog writes it.
pub fn into_folder(path: &Path) -> String {
    let mut text = path.display().to_string();
    if !text.ends_with(std::path::is_separator) {
        text.push(MAIN_SEPARATOR);
    }
    text
}

#[cfg(test)]
#[path = "dropped_tests.rs"]
mod tests;
