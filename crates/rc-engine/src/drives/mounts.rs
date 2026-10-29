//! The mount points of `/proc/mounts` that belong in the drive bar outside Windows: those under
//! `/mnt`, `/media` and `/run/media`, and network file systems wherever they are mounted. The
//! system's own mounts (`/proc`, `/sys`, tmpfs, cgroups, WSL's helpers) stay out.

use std::path::PathBuf;

use super::{DriveKind, RawDrive};

/// File systems on another machine.
const NETWORK: &[&str] = &[
    "afs",
    "cifs",
    "fuse.sshfs",
    "ncpfs",
    "nfs",
    "nfs4",
    "smb3",
    "smbfs",
];

/// Where removable media are mounted.
const MEDIA: &[&str] = &["/media/", "/run/media/"];

/// The drives of `text`, the content of `/proc/mounts`, in its order, each mount point once.
pub(crate) fn parse(text: &str) -> Vec<RawDrive> {
    let mut drives: Vec<RawDrive> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(_device), Some(point), Some(fs_type)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let point = unescape(point);
        let network = NETWORK.contains(&fs_type);
        let media = MEDIA.iter().any(|prefix| point.starts_with(prefix));
        let mounted = point.starts_with("/mnt/") && !point.starts_with("/mnt/wsl");
        if !(network || media || mounted) || drives.iter().any(|drive| drive.id == point) {
            continue;
        }
        let kind = if network {
            DriveKind::Network
        } else if media {
            DriveKind::Removable
        } else {
            DriveKind::Fixed
        };
        let label = point
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(&point)
            .to_owned();
        drives.push(RawDrive {
            id: point.clone(),
            label,
            kind,
            root: PathBuf::from(point),
            remote: None,
        });
    }
    drives
}

/// A field of `/proc/mounts` with its octal escapes (`\040` for a space) undone.
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut rest = field;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let code = rest.get(at + 1..at + 4);
        match code.and_then(|digits| u8::from_str_radix(digits, 8).ok()) {
            Some(byte) => {
                out.push(char::from(byte));
                rest = &rest[at + 4..];
            }
            None => {
                out.push('\\');
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "mounts_tests.rs"]
mod tests;
