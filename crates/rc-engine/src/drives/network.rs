//! Network drives: a share mapped to a drive letter and disconnected again (RC-010-16), as Total
//! Commander's *Net - Connect network drive / Disconnect network drive* do it, through Windows'
//! own functions.
//!
//! Mapping goes through `WNetAddConnection2W` with `CONNECT_INTERACTIVE | CONNECT_PROMPT`: Windows
//! asks for the user and the password itself and keeps them in its Credential Manager, so
//! rCommander never touches a password (and never runs `net use`, which shows one in the process
//! list). Disconnecting is `WNetCancelConnection2W`; open files make Windows refuse, and only a
//! second call with `force` disconnects anyway. The calls are `drives/wnet.rs`, Windows only;
//! elsewhere [`super::DriveSource`] answers [`UNSUPPORTED`].
//!
//! What the commands take is checked here first, on every platform: a folder `\\server\share`,
//! deeper folders allowed, and a drive letter A to Z that no drive has. What Windows answers is a
//! Win32 error code; [`from_system`] turns it into the engine's error.

use thiserror::Error;

use super::{Drive, DriveGroup, letter_id};
use crate::EngineError;

/// Windows' `ERROR_NOT_SUPPORTED`: what a platform without network drives answers.
pub(crate) const UNSUPPORTED: u32 = 50;

/// The longest folder accepted, in characters.
const MAX_REMOTE: usize = 1024;

/// Characters no server, share or folder name has.
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*', '/'];

/// A network drive refused or failed; its code is stable.
#[derive(Debug, Error)]
pub enum NetworkDriveError {
    /// The folder is not `\\server\share[\...]`.
    #[error("{reason}; a network folder is written \\\\server\\share")]
    PathInvalid { reason: &'static str },
    /// Not a drive letter A to Z.
    #[error("{letter:?} is not a drive letter from A to Z")]
    LetterInvalid { letter: String },
    /// A drive already has the letter.
    #[error("drive {letter}: is already in use")]
    LetterInUse { letter: char },
    /// The drive to disconnect is not a mapped network drive.
    #[error("drive {drive} is not a network drive")]
    NotNetwork { drive: String },
    /// Files on the drive are open; only a forced disconnect closes them.
    #[error("files on drive {drive} are open; disconnecting anyway may lose unsaved data")]
    InUse { drive: String },
    /// The server or the share cannot be found.
    #[error("the network folder {remote} cannot be found")]
    PathNotFound { remote: String },
    /// Windows is connected to the server already, under another user.
    #[error(
        "Windows is already connected to the server of {remote} as another user; disconnect \
         that connection first"
    )]
    CredentialConflict { remote: String },
    /// The server refused the user Windows asked for.
    #[error("the server refused access to {remote}")]
    AccessDenied { remote: String },
    /// This platform maps no network drives.
    #[error("network drives can only be mapped on Windows")]
    Unsupported,
    /// Any other answer of Windows.
    #[error("Windows answered error {code}")]
    Failed { code: u32 },
}

impl NetworkDriveError {
    /// Every code.
    pub const ALL_CODES: &[&str] = &[
        "network_path_invalid",
        "drive_letter_invalid",
        "drive_letter_in_use",
        "drive_not_network",
        "network_drive_in_use",
        "network_path_not_found",
        "network_credentials_conflict",
        "network_access_denied",
        "network_drives_unsupported",
        "network_drive_failed",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathInvalid { .. } => "network_path_invalid",
            Self::LetterInvalid { .. } => "drive_letter_invalid",
            Self::LetterInUse { .. } => "drive_letter_in_use",
            Self::NotNetwork { .. } => "drive_not_network",
            Self::InUse { .. } => "network_drive_in_use",
            Self::PathNotFound { .. } => "network_path_not_found",
            Self::CredentialConflict { .. } => "network_credentials_conflict",
            Self::AccessDenied { .. } => "network_access_denied",
            Self::Unsupported => "network_drives_unsupported",
            Self::Failed { .. } => "network_drive_failed",
        }
    }
}

impl From<NetworkDriveError> for EngineError {
    fn from(error: NetworkDriveError) -> Self {
        Self::NetworkDrive(error)
    }
}

/// `text` as a network folder `\\server\share[\...]`, with `/` read as `\` and a trailing `\`
/// dropped. A device path (`\\?\`, `\\.\`), an empty part, `.` or `..`, and characters no name
/// has are refused.
///
/// # Errors
///
/// [`NetworkDriveError::PathInvalid`] with the reason.
pub fn parse_remote(text: &str) -> Result<String, NetworkDriveError> {
    let invalid = |reason| Err(NetworkDriveError::PathInvalid { reason });
    let text = text.trim().replace('/', "\\");
    if text.chars().count() > MAX_REMOTE {
        return invalid("the folder is too long");
    }
    let Some(rest) = text.strip_prefix(r"\\") else {
        return invalid("a network folder starts with two backslashes");
    };
    let parts: Vec<&str> = rest.trim_end_matches('\\').split('\\').collect();
    if parts.len() < 2 {
        return invalid("a network folder names a server and a share");
    }
    if matches!(parts[0], "?" | ".") {
        return invalid("a device path is not a network folder");
    }
    for part in &parts {
        if part.is_empty() {
            return invalid("a part of the folder is empty");
        }
        if matches!(*part, "." | "..") {
            return invalid("a part of the folder is . or ..");
        }
        if part
            .chars()
            .any(|c| c.is_control() || FORBIDDEN.contains(&c))
        {
            return invalid("the folder holds a character no name can have");
        }
    }
    Ok(format!(r"\\{}", parts.join(r"\")))
}

/// `text` (`z`, `Z:`, `z:\`) as a drive letter no entry of `drives` has.
///
/// # Errors
///
/// [`NetworkDriveError::LetterInvalid`] for anything but a letter A to Z,
/// [`NetworkDriveError::LetterInUse`] for a letter a drive has.
pub fn parse_letter(text: &str, drives: &[Drive]) -> Result<char, NetworkDriveError> {
    let letter = letter_id(text.trim())
        .and_then(|id| id.chars().next())
        .ok_or_else(|| NetworkDriveError::LetterInvalid {
            letter: text.to_owned(),
        })?;
    if taken(drives).contains(&letter) {
        return Err(NetworkDriveError::LetterInUse { letter });
    }
    Ok(letter)
}

/// The letter the map dialog proposes: the last free one from Z down to D, as Windows' own
/// dialog does; `None` when all are taken.
#[must_use]
pub fn suggest_letter(drives: &[Drive]) -> Option<char> {
    let taken = taken(drives);
    ('D'..='Z').rev().find(|letter| !taken.contains(letter))
}

/// The letters the drives of `drives` have.
fn taken(drives: &[Drive]) -> Vec<char> {
    drives
        .iter()
        .filter(|drive| drive.group == DriveGroup::Drives)
        .filter_map(|drive| letter_id(&drive.id))
        .filter_map(|id| id.chars().next())
        .collect()
}

/// Windows' answer `code` to mapping `remote` to `letter` or disconnecting `letter`, as the
/// engine's error. The person closing Windows' credential dialog is [`EngineError::Cancelled`].
pub(crate) fn from_system(code: u32, letter: char, remote: &str) -> EngineError {
    let drive = format!("{letter}:");
    let remote = remote.to_owned();
    let error = match code {
        // ERROR_CANCELLED
        1223 => return EngineError::Cancelled,
        // ERROR_ALREADY_ASSIGNED, ERROR_DEVICE_ALREADY_REMEMBERED
        85 | 1202 => NetworkDriveError::LetterInUse { letter },
        // ERROR_BAD_DEVICE
        1200 => NetworkDriveError::LetterInvalid { letter: drive },
        // ERROR_BAD_NETPATH, ERROR_BAD_NET_NAME, ERROR_NO_NET_OR_BAD_PATH
        53 | 67 | 1203 => NetworkDriveError::PathNotFound { remote },
        // ERROR_SESSION_CREDENTIAL_CONFLICT
        1219 => NetworkDriveError::CredentialConflict { remote },
        // ERROR_ACCESS_DENIED, ERROR_INVALID_PASSWORD, ERROR_LOGON_FAILURE
        5 | 86 | 1326 => NetworkDriveError::AccessDenied { remote },
        // ERROR_OPEN_FILES, ERROR_DEVICE_IN_USE
        2401 | 2404 => NetworkDriveError::InUse { drive },
        // ERROR_NOT_CONNECTED
        2250 => NetworkDriveError::NotNetwork { drive },
        UNSUPPORTED => NetworkDriveError::Unsupported,
        code => NetworkDriveError::Failed { code },
    };
    error.into()
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
