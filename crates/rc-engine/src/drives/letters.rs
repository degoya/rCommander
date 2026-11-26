//! The drive letters as Windows reports them (`windows.rs`), apart from the calls so the rules
//! are tested on every platform (RC-110-70): the letters of `GetLogicalDrives`' mask, a letter's
//! drive, and the remembered network drives without a letter yet.

use std::path::PathBuf;

use super::{DriveKind, RawDrive};

/// The letters whose bits `mask` sets, `A` for bit 0 to `Z` for bit 25; higher bits are no
/// letter.
pub(super) fn present_letters(mask: u32) -> Vec<char> {
    (0_u8..26)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| char::from(b'A' + bit))
        .collect()
}

/// The drive of `letter`: `C:` with the root `C:\`.
pub(super) fn raw_drive(letter: char, kind: DriveKind, remote: Option<String>) -> RawDrive {
    RawDrive {
        id: format!("{letter}:"),
        label: format!("{letter}:"),
        kind,
        root: PathBuf::from(format!("{letter}:\\")),
        remote,
    }
}

/// `drives` with each `remembered` network drive whose letter none of them has, as a network
/// drive with its folder, sorted by letter.
pub(super) fn with_remembered(
    mut drives: Vec<RawDrive>,
    remembered: impl IntoIterator<Item = (char, String)>,
) -> Vec<RawDrive> {
    for (letter, remote) in remembered {
        if !drives.iter().any(|drive| drive.id.starts_with(letter)) {
            drives.push(raw_drive(letter, DriveKind::Network, Some(remote)));
        }
    }
    drives.sort_by(|left, right| left.id.cmp(&right.id));
    drives
}

#[cfg(test)]
#[path = "letters_tests.rs"]
mod tests;
