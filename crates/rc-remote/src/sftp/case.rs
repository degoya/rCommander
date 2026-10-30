//! Whether the server tells names apart by case (RC-080-60). SFTP does not say: an OpenSSH
//! server on NTFS or APFS folds case, one on ext4 does not. [`SftpCase::Detect`] asks once, when
//! the backend opens: the home directory is `stat`ed as named and with one of its names in the
//! other case (the last one with an ASCII letter), both requests at once, so the probe costs one
//! round trip and writes nothing. The same object under both names (kind, size, modification
//! time) means the server folds case; a home without letters, a name the server does not find or
//! any error means it does not - what the backend claimed before the probe.

use russh_sftp::{client::RawSftpSession, protocol::FileAttributes};

/// How the backend learns whether the server tells names apart by case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SftpCase {
    /// Probe the server when the backend opens.
    #[default]
    Detect,
    /// Names that differ only in case are different names.
    Sensitive,
    /// Names that differ only in case are one name.
    Insensitive,
}

/// `wire` with the last of its names that holds an ASCII letter in the other case (`/home/Tester`
/// to `/home/tESTER`); `None` when no name holds one.
pub(super) fn flipped(wire: &str) -> Option<String> {
    let mut names: Vec<String> = wire.split('/').map(str::to_owned).collect();
    let name = names
        .iter_mut()
        .rev()
        .find(|name| name.bytes().any(|byte| byte.is_ascii_alphabetic()))?;
    *name = name
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    Some(names.join("/"))
}

/// Whether two answers describe the same object: kind, size and modification time.
pub(super) fn same_object(named: &FileAttributes, other: &FileAttributes) -> bool {
    named.file_type() == other.file_type() && named.size == other.size && named.mtime == other.mtime
}

/// Whether the server at `raw` tells names apart by case, probed on `home` (a wire path).
pub(super) async fn detect(raw: &RawSftpSession, home: &str) -> bool {
    let Some(other) = flipped(home) else {
        return true;
    };
    let (named, other) = tokio::join!(raw.stat(home), raw.stat(other));
    match (named, other) {
        (Ok(named), Ok(other)) => !same_object(&named.attrs, &other.attrs),
        _ => true,
    }
}

#[cfg(test)]
#[path = "case_tests.rs"]
mod tests;
