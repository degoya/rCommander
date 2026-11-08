//! Whether a panel's folder is in a git repository, and where git may run for it (RC-100-97).
//!
//! rCommander looks itself: from the folder up to the drive's root for `.git`, a folder or - in
//! a linked worktree or a submodule - a file. Those are `stat` calls only; outside a repository
//! no process starts. Only folders of this machine's backend qualify: a local drive (a network
//! drive or share only with `local_network`) or a WSL distribution's share, never a server, an
//! archive or a device.

use std::path::{Component, Path, PathBuf};

use rc_vfs::{BackendId, VfsPath, WslPath, parse_wsl_unc};

use super::GitScope;

/// Where git runs for a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    /// This machine's git.
    Local,
    /// The git of the WSL distribution whose share holds the folder.
    Wsl(WslPath),
}

/// Where git may run for `path` under `scope`; `network` says the folder lies on a network
/// drive or share. `None`: nowhere.
pub(crate) fn place(path: &VfsPath, scope: GitScope, network: bool) -> Option<Place> {
    if path.backend != BackendId::Local || scope == GitScope::Off {
        return None;
    }
    // A distribution's share is this machine's, whatever its form says.
    if let Some(wsl) = parse_wsl_unc(&path.path) {
        return Some(Place::Wsl(wsl));
    }
    if network && scope != GitScope::LocalNetwork {
        return None;
    }
    Some(Place::Local)
}

/// The repository's root above `dir`: the nearest of `dir` and its parents holding `.git`, up to
/// the drive's root; `None` outside a repository.
pub(crate) fn repository_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|folder| holds_git(folder))
        .map(Path::to_path_buf)
}

/// Whether `folder` holds `.git` as a folder or a file.
fn holds_git(folder: &Path) -> bool {
    std::fs::metadata(folder.join(".git")).is_ok_and(|meta| meta.is_dir() || meta.is_file())
}

/// `dir` relative to `root`, `/`-separated; empty for the root itself. `None` when `dir` is not
/// below `root` or names a part that is no plain name.
pub(crate) fn relative(root: &Path, dir: &Path) -> Option<String> {
    let rest = dir.strip_prefix(root).ok()?;
    let parts = rest
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str().map(str::to_owned),
            _ => None,
        })
        .collect::<Option<Vec<String>>>()?;
    Some(parts.join("/"))
}

#[cfg(test)]
#[path = "discover_tests.rs"]
mod tests;
