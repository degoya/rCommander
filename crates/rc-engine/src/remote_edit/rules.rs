//! The rules of a remote edit that need no engine: which server file would run as a program,
//! which one is written in place, where a link leads, the stamp of a local copy.

use std::path::{Component, Path, PathBuf};

use rc_vfs::{Details, Entry};

use super::Stamp;

/// The extensions Windows runs when `PATHEXT` is not set.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC";

/// Whether Enter on the server file `name` with the Unix mode `mode` would start a program:
/// on Windows an extension of `pathext` (`PATHEXT`, case does not count), elsewhere an execute
/// bit.
#[must_use]
pub(crate) fn runs_as_program(name: &str, mode: Option<u32>, windows: bool, pathext: &str) -> bool {
    if windows {
        let Some((_, extension)) = name.rsplit_once('.') else {
            return false;
        };
        let extension = format!(".{extension}");
        return pathext
            .split(';')
            .map(str::trim)
            .any(|listed| !listed.is_empty() && listed.eq_ignore_ascii_case(&extension));
    }
    mode.is_some_and(|mode| mode & 0o111 != 0)
}

/// [`runs_as_program`] on this platform, with its `PATHEXT`.
#[must_use]
pub(crate) fn runs_here(name: &str, mode: Option<u32>) -> bool {
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| DEFAULT_PATHEXT.to_owned());
    runs_as_program(name, mode, cfg!(windows), &pathext)
}

/// Whether an upload must write into the server's file itself rather than replace it: it has
/// several names (hard links), or the listing names an owner other than the connection's user -
/// a replacement through a part file would cut the links or take the file over. An owner given
/// as a number (an SFTP `stat`) is not compared: the user's number is not known.
#[must_use]
pub(crate) fn needs_in_place(entry: &Entry, details: &Details, user: Option<&str>) -> bool {
    if details.links.is_some_and(|links| links > 1) {
        return true;
    }
    let (Some(owner), Some(user)) = (&entry.owner, user) else {
        return false;
    };
    let numeric = owner.user.bytes().all(|byte| byte.is_ascii_digit());
    !numeric && !owner.user.is_empty() && owner.user != user
}

/// Where the link at `link` with the stored target `target` leads: an absolute target as it
/// is, a relative one from the link's folder; `.` and `..` resolved, never above the root.
#[must_use]
pub(crate) fn link_destination(link: &Path, target: &Path) -> PathBuf {
    let joined = if target.has_root() {
        target.to_path_buf()
    } else {
        link.parent().unwrap_or(Path::new("/")).join(target)
    };
    let mut resolved = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // The root's parent is the root.
                if resolved.parent().is_some() {
                    resolved.pop();
                }
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved
}

/// The stamp of the local copy `file`; `None` while it cannot be read. Blocking.
#[must_use]
pub(crate) fn local_stamp(file: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(file).ok()?;
    metadata.is_file().then(|| Stamp {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// Marks the fetched copy `file` as coming from the internet (`Zone.Identifier`, zone 3), so
/// SmartScreen and the Office protected view treat it as such. Only Windows has the stream; a
/// file system without streams (FAT) leaves the copy unmarked.
#[cfg(windows)]
pub(crate) fn mark_from_internet(file: &Path) {
    let mut stream = file.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    if let Err(error) = std::fs::write(&stream, b"[ZoneTransfer]\r\nZoneId=3\r\n") {
        tracing::warn!(%error, "a fetched server file was not marked as coming from the internet");
    }
}

/// Nothing outside Windows: no zone streams there.
#[cfg(not(windows))]
pub(crate) fn mark_from_internet(_file: &Path) {}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
