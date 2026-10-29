//! The drive bar's entries: the drives, the WSL distributions and the saved servers.
//!
//! Total Commander's drive bar shows every drive letter with its type, and the free space of the
//! drive a panel is on; rCommander adds a group for the WSL distributions (`rc_vfs::wsl`) and one
//! for the saved servers (the connection tree, each server with its folders, and the quick
//! connections of this run); an SSH server opens over SFTP (RC-060-02).
//!
//! Which drives exist and of what kind is cheap to ask and never touches a drive (on Windows
//! `GetLogicalDrives` and `GetDriveTypeW`, `drives/windows.rs`; elsewhere `/`, the home directory
//! and the mount points of `/proc/mounts`, `drives/unix.rs`). The volume name and the space are
//! not: an empty CD drive, a sleeping disk or a disconnected network share can take seconds or
//! hang. [`DriveCatalog`] asks for them off the UI thread, each at most [`DETAIL_TIMEOUT`], keeps
//! the answers, never asks a drive again while it has not answered, and asks a network drive only
//! when someone wants it (`drives.list` with `refresh`, or a panel changing to it) - a refresh of
//! the bar never wakes a sleeping share. A late answer is announced as
//! [`crate::EngineEvent::Drives`].
//!
//! A panel remembers the last directory it showed on every drive (Total Commander does the same):
//! [`drive_key`] names the drive of a path, by its prefix on Windows (`C:`, `\\server\share`) and
//! by the longest drive root elsewhere, and a server's by the server.
//!
//! A network drive shows the folder it is mapped to (`\\server\share`, Windows'
//! `WNetGetConnectionW`); mapping one and disconnecting it is `drives/network.rs` (RC-010-16).

mod catalog;
#[cfg(test)]
pub(crate) mod fake;
// The parsing of `/proc/mounts` compiles and is tested everywhere; only Unix reads the file.
#[cfg_attr(windows, allow(dead_code, reason = "only Unix reads /proc/mounts"))]
mod mounts;
pub(crate) mod network;
#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod wnet;

use std::{
    io,
    path::{Component, Path, PathBuf, Prefix},
    sync::Arc,
    time::Duration,
};

use rc_vfs::{BackendId, VfsPath};
use serde::{Deserialize, Serialize};

pub(crate) use catalog::DriveCatalog;
pub use network::{NetworkDriveError, suggest_letter};

/// How long a drive may take to tell its volume name and space before it counts as waiting.
pub const DETAIL_TIMEOUT: Duration = Duration::from_secs(2);

/// Whether this platform maps network drives (`drives.map_network`, `drives.unmap_network`);
/// elsewhere both answer `network_drives_unsupported` and the window offers neither.
pub const NETWORK_DRIVES: bool = cfg!(windows);

/// The group of the drive bar an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveGroup {
    /// Drive letters (Windows), the root, the home directory and mount points (elsewhere).
    Drives,
    /// The installed WSL distributions.
    Wsl,
    /// The saved servers of the connection tree, and the quick connections.
    Servers,
}

/// What kind of drive an entry is; the drive bar draws a symbol for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    /// CD, DVD or Blu-ray.
    Optical,
    Ram,
    /// A drive the system does not classify.
    Unknown,
    /// A WSL distribution.
    Wsl,
    /// A saved server connection.
    Server,
}

/// Bytes on a drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveSpace {
    /// Free for this user.
    pub free: u64,
    pub total: u64,
}

/// What is known of a drive's volume name and space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveState {
    /// It answered: volume name and space are known.
    Ready,
    /// It answered that it cannot be read: no medium, or the share is not reachable.
    NotReady,
    /// It was asked and has not answered within [`DETAIL_TIMEOUT`]; the answer is announced
    /// when it comes.
    Waiting,
    /// Not asked: a network drive nobody wanted yet, a WSL distribution, a server.
    Unknown,
}

/// One entry of the drive bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drive {
    /// What `panel.change_drive` takes: `C:` or `\\server\share` on Windows, the mount point
    /// elsewhere, `server:<connection id>` for a server.
    pub id: String,
    pub group: DriveGroup,
    pub kind: DriveKind,
    /// The short name the bar shows: `C:`, the distribution's or the connection's name.
    pub label: String,
    /// Where a panel goes: `/` on an SSH server (`panel.change_drive` starts in its home);
    /// `None` for an FTP server, which has no panel yet.
    pub root: Option<VfsPath>,
    pub state: DriveState,
    /// The volume name, when the drive has one and answered.
    pub volume: Option<String>,
    pub space: Option<DriveSpace>,
    /// A server's folders in the connection tree, from the root down; empty at the root and for
    /// every other entry. The drive bar shows a folder as a button that opens the menu at its
    /// servers, every one of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folder: Vec<String>,
    /// A server the drive bar shows as a button of its own: a connection the manager marks
    /// (RC-050-04; by default those of the tree's root level) or a quick connection. The drive
    /// menu lists every server, marked or not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favourite: bool,
    /// The folder a network drive is mapped to (`\\server\share`), also while it is not
    /// connected; `None` for every other entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

/// The entry of `drives` the location `path` lies on: the one whose key [`drive_key`] names.
#[must_use]
pub fn drive_of<'a>(drives: &'a [Drive], path: &VfsPath) -> Option<&'a Drive> {
    let key = drive_key(path, &roots_of(drives))?;
    drives.iter().find(|drive| drive.id == key)
}

/// The id and local root of every entry that has one, for [`drive_key`].
pub(crate) fn roots_of(drives: &[Drive]) -> Vec<(String, PathBuf)> {
    drives
        .iter()
        .filter_map(|drive| match &drive.root {
            Some(VfsPath {
                backend: BackendId::Local,
                path,
            }) => Some((drive.id.clone(), path.clone())),
            _ => None,
        })
        .collect()
}

/// The key of the drive `path` lies on: a Windows path's drive letter (`C:`) or share
/// (`\\server\share`, in lower case); elsewhere the id of the longest root in `roots` above it.
/// A location inside an archive lies on the archive's drive, one over SFTP or FTP on its
/// server's (`server:<connection id>`). `None` for a path on no drive.
pub(crate) fn drive_key(path: &VfsPath, roots: &[(String, PathBuf)]) -> Option<String> {
    if let BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } = path.backend {
        return Some(format!("server:{connection_id}"));
    }
    let path = local_path(path)?;
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        return Some(
            prefix_key(prefix.kind())
                .unwrap_or_else(|| prefix.as_os_str().to_string_lossy().into_owned()),
        );
    }
    roots
        .iter()
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.components().count())
        .map(|(id, _)| id.clone())
}

fn local_path(path: &VfsPath) -> Option<&Path> {
    match &path.backend {
        BackendId::Local => Some(&path.path),
        BackendId::Archive { outer, .. } => local_path(outer),
        _ => None,
    }
}

fn prefix_key(prefix: Prefix<'_>) -> Option<String> {
    match prefix {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            Some(format!("{}:", char::from(letter).to_ascii_uppercase()))
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => Some(
            format!(
                r"\\{}\{}",
                server.to_string_lossy(),
                share.to_string_lossy()
            )
            .to_lowercase(),
        ),
        _ => None,
    }
}

/// The entry `wanted` names: its id exactly, a drive letter however written (`c`, `C:`, `c:\`),
/// or an id or label ignoring case (a distribution's name).
pub(crate) fn find<'a>(drives: &'a [Drive], wanted: &str) -> Option<&'a Drive> {
    let wanted = wanted.trim();
    let letter = letter_id(wanted);
    drives
        .iter()
        .find(|drive| drive.id == wanted || letter.as_deref() == Some(drive.id.as_str()))
        .or_else(|| {
            drives.iter().find(|drive| {
                drive.id.eq_ignore_ascii_case(wanted) || drive.label.eq_ignore_ascii_case(wanted)
            })
        })
}

/// `c`, `C:`, `c:\` and `C:/` as the id `C:`.
fn letter_id(text: &str) -> Option<String> {
    let bare = text.trim_end_matches(['\\', '/']);
    let bare = bare.strip_suffix(':').unwrap_or(bare);
    let mut chars = bare.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) if letter.is_ascii_alphabetic() => {
            Some(format!("{}:", letter.to_ascii_uppercase()))
        }
        _ => None,
    }
}

/// A drive as the system lists it, before anyone asked it anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawDrive {
    pub id: String,
    pub label: String,
    pub kind: DriveKind,
    pub root: PathBuf,
    /// A network drive's folder.
    pub remote: Option<String>,
}

/// What a drive tells when asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Details {
    pub volume: Option<String>,
    pub space: Option<DriveSpace>,
}

/// Where the drives come from: the system, or a test's stand-in.
pub(crate) trait DriveSource: Send + Sync {
    /// The drives there now, without touching any of them.
    fn drives(&self) -> Vec<RawDrive>;

    /// The volume name and space of the drive at `root`; may block as long as the drive takes.
    fn details(&self, root: &Path) -> io::Result<Details>;

    /// Maps `remote` to the drive `letter`, Windows asking for the credentials; blocks until
    /// the person answered. The error is Windows' code, [`network::UNSUPPORTED`] where there
    /// are no network drives.
    fn map_network(&self, _letter: char, _remote: &str, _reconnect: bool) -> Result<(), u32> {
        Err(network::UNSUPPORTED)
    }

    /// Disconnects the network drive `letter`; without `force` refused while files on it are
    /// open. The error is Windows' code.
    fn unmap_network(&self, _letter: char, _force: bool) -> Result<(), u32> {
        Err(network::UNSUPPORTED)
    }

    /// Connects the remembered network drive `letter` that is not connected, as Explorer does
    /// on its first use; blocks while Windows asks for credentials it does not have. The error
    /// is Windows' code.
    fn restore_network(&self, _letter: char) -> Result<(), u32> {
        Err(network::UNSUPPORTED)
    }
}

/// The system's drives.
pub(crate) fn system() -> Arc<dyn DriveSource> {
    #[cfg(windows)]
    let source = windows::System;
    #[cfg(not(windows))]
    let source = unix::System;
    Arc::new(source)
}

/// The space of the file system at `root`, the call `Vfs::free_space` makes too.
fn space(root: &Path) -> io::Result<DriveSpace> {
    let (free, total) = rc_vfs::disk_space(root)?;
    Ok(DriveSpace { free, total })
}

#[cfg(test)]
#[path = "drives_tests.rs"]
mod tests;
