//! Network shares on macOS (RC-110-40), the parts that compile and are tested everywhere: the
//! `smb://` address of a share in both directions, the lines of the system's mount table
//! (`/sbin/mount`) and the error number osascript ends its complaint with. The calls themselves
//! are `drives/macos.rs`.

use std::path::Path;

use super::network::NetworkDriveError;

/// What a source that mounts no shares answers; no AppleScript error and no errno has it.
pub(crate) const MOUNT_UNSUPPORTED: i32 = i32::MIN;

/// The file systems of macOS's mount table that live on another machine.
const NETWORK: &[&str] = &["smbfs", "afpfs", "nfs", "webdav"];

/// The scheme of a share's address.
const SCHEME: &str = "smb://";

/// `text` without its leading `smb://` (in any case); `None` when it has none.
pub(crate) fn strip_scheme(text: &str) -> Option<&str> {
    let head = text.get(..SCHEME.len())?;
    let rest = text.get(SCHEME.len()..)?;
    head.eq_ignore_ascii_case(SCHEME).then_some(rest)
}

/// The address after `smb://` as the folder `\\server\share[\...]`, its percent escapes undone;
/// what the parts may hold is checked afterwards, as for a folder written with backslashes.
///
/// # Errors
///
/// [`NetworkDriveError::PathInvalid`] for a user name before the server (the system asks for it,
/// and a password must never travel in the address), an escape that is no text, and an escaped
/// slash or backslash.
pub(crate) fn unc_of(address: &str) -> Result<String, NetworkDriveError> {
    let invalid = |reason| Err(NetworkDriveError::PathInvalid { reason });
    let server = address.split('/').next().unwrap_or_default();
    if server.contains('@') {
        return invalid("a user name does not belong in the folder; the system asks for it");
    }
    let mut parts = Vec::new();
    for part in address.split('/') {
        let Some(part) = decode(part) else {
            return invalid("the folder holds an escape that is no text");
        };
        if part.contains(['/', '\\']) {
            return invalid("the folder holds a character no name can have");
        }
        parts.push(part);
    }
    Ok(format!(r"\\{}", parts.join(r"\")))
}

/// The address macOS mounts the share `remote` from (`\\server\share`, as
/// [`super::network::parse_remote`] answers it): `smb://server/share`, the share percent-encoded.
///
/// # Errors
///
/// [`NetworkDriveError::PathInvalid`] for a folder below the share (macOS mounts a share, not a
/// folder in it) and for a server name no host has.
pub(crate) fn url(remote: &str) -> Result<String, NetworkDriveError> {
    let invalid = |reason| Err(NetworkDriveError::PathInvalid { reason });
    let parts: Vec<&str> = remote.trim_start_matches('\\').split('\\').collect();
    let [server, share] = parts.as_slice() else {
        return invalid("macOS mounts a share, not a folder below it");
    };
    let host = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    if server.is_empty() || !server.chars().all(host) {
        return invalid("the server name holds a character no host name has");
    }
    Ok(format!("{SCHEME}{server}/{}", encode(share)))
}

/// The share `remote` (`\\server\share`) as a macOS user writes it, for messages:
/// `smb://server/share`.
pub(crate) fn display(remote: &str) -> String {
    format!(
        "{SCHEME}{}",
        remote.trim_start_matches('\\').replace('\\', "/")
    )
}

/// A line of the mount table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MountEntry {
    /// What is mounted: `//user@server/share` for an SMB share, a device otherwise.
    pub from: String,
    pub point: String,
    pub fs_type: String,
}

/// The lines of `/sbin/mount`'s answer, `<from> on <point> (<type>, <options>)`; a line that
/// does not read so is left out.
pub(crate) fn parse_table(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let (head, options) = line.trim_end().strip_suffix(')')?.rsplit_once(" (")?;
            let (from, point) = head.split_once(" on /")?;
            let fs_type = options.split(',').next()?.trim();
            Some(MountEntry {
                from: from.to_owned(),
                point: format!("/{point}"),
                fs_type: fs_type.to_owned(),
            })
        })
        .collect()
}

/// The folder of the network file system mounted at `point`, as the drive bar shows it:
/// `smb://server/share` for an SMB share (the user name left out, escapes undone), what the table
/// names for another one; `None` when no network file system is mounted there.
pub(crate) fn remote_at(point: &Path, table: &[MountEntry]) -> Option<String> {
    let entry = table
        .iter()
        .rev()
        .find(|entry| Path::new(&entry.point) == point)?;
    if !NETWORK.contains(&entry.fs_type.as_str()) {
        return None;
    }
    if entry.fs_type != "smbfs" {
        return Some(entry.from.clone());
    }
    let address = entry.from.trim_start_matches('/');
    let (authority, path) = address.split_once('/').unwrap_or((address, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let path: Vec<String> = path
        .split('/')
        .map(|part| decode(part).unwrap_or_else(|| part.to_owned()))
        .collect();
    Some(format!("{SCHEME}{host}/{}", path.join("/")))
}

/// The error number osascript ends its complaint with: `0:42: execution error: User canceled.
/// (-128)`; `None` when there is none.
pub(crate) fn script_error(stderr: &str) -> Option<i32> {
    let text = stderr.trim_end().strip_suffix(')')?;
    let (_, number) = text.rsplit_once('(')?;
    number.trim().parse().ok()
}

/// `text` with its `%XX` escapes undone; `None` when the bytes are no UTF-8. A `%` that starts no
/// escape stays.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let escaped = text
            .get(at + 1..at + 3)
            .filter(|hex| byte == b'%' && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(value) => {
                out.push(value);
                at += 3;
            }
            None => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// `text` percent-encoded but for the characters an address keeps as they are.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
