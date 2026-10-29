//! What a new name must be, and whether the file system tells upper from lower case.
//!
//! A mask and a replacement are untrusted input (they also come over MCP),
//! so every new name is checked against the Windows rules on every file system: no separator,
//! no `.` or `..`, no character or device name Windows refuses, no character that turns the
//! reading direction, no trailing dot or space. A name created on ext4 stays usable from Windows
//! (`\\wsl.localhost`) and a share.
//!
//! The one name rule of rCommander (RC-080-43): [`check_plain`] is what `rc-ops` (a typed
//! "rename to", a name a copy gives a Windows folder, an archive entry unpacked) and
//! `rc-checksum` (a combined file, split parts, a checksum file) hold a name to.

use serde::{Deserialize, Serialize};

use crate::turns_direction;

/// The naming rules of the file system a plan renames on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRules {
    /// How name and path lengths are counted.
    pub names: NameRules,
    /// `readme.txt` and `README.txt` are the same entry (NTFS, FAT, exFAT, APFS by default).
    pub case_insensitive: bool,
    /// The longest full path, in UTF-16 units under [`NameRules::Windows`] and bytes under
    /// [`NameRules::Unix`].
    pub max_path: usize,
}

impl FsRules {
    /// NTFS through long paths (`\\?\`), which is how `rc-vfs` opens local Windows paths.
    pub const WINDOWS: Self = Self {
        names: NameRules::Windows,
        case_insensitive: true,
        max_path: 32_767,
    };

    /// A Linux file system: case-sensitive, lengths in bytes.
    pub const UNIX: Self = Self {
        names: NameRules::Unix,
        case_insensitive: false,
        max_path: 4_095,
    };

    /// The rules of the platform rCommander runs on, for its local files.
    #[must_use]
    pub fn native() -> Self {
        if cfg!(windows) {
            Self::WINDOWS
        } else {
            Self::UNIX
        }
    }

    /// The form two names are compared in: lower case where the file system ignores case.
    ///
    /// Unicode lower case is close to, not identical with, NTFS's upcase table; it differs only
    /// for a few letters no file name in practice depends on.
    #[must_use]
    pub fn key(&self, name: &str) -> String {
        if self.case_insensitive {
            name.to_lowercase()
        } else {
            name.to_owned()
        }
    }
}

/// How a file system counts the length of names and paths; which names are refused is the
/// same for both (see the module).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameRules {
    /// UTF-16 units, 255 per name.
    Windows,
    /// Bytes, 255 per name.
    Unix,
}

/// Why a new name cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum InvalidName {
    /// Nothing is left of the name.
    Empty,
    /// `.` or `..`.
    DotName,
    /// A `/` or `\` in one name; in a new name of a plan they move the entry into a subfolder
    /// instead ([`crate::check_path`]).
    Separator,
    /// A character Windows refuses in names.
    ReservedChar { ch: char },
    /// A control character.
    ControlChar,
    /// A character that turns the reading direction ([`crate::turns_direction`]):
    /// `invoice<U+202E>fdp.exe` reads as `invoiceexe.pdf`.
    TurnsDirection,
    /// A device name such as `CON` or `COM1`, with or without an extension.
    ReservedName,
    /// A dot or a space at the end, which Windows drops.
    TrailingDotOrSpace,
    /// Longer than 255 UTF-16 units (Windows) or bytes (Unix).
    NameTooLong,
    /// The directory and the name together are longer than [`FsRules::max_path`].
    PathTooLong,
    /// A subfolder path that climbs out of the directory (`..\x`).
    ParentDir,
    /// A subfolder path that starts at a root (`\x`).
    AbsolutePath,
    /// A subfolder path that names a drive (`C:\x`).
    DrivePath,
    /// A subfolder path that names a share (`\\server\x`).
    UncPath,
    /// A subfolder path with an empty part (`a\\b`, `a\`).
    EmptyPart,
}

impl InvalidName {
    /// Every code [`InvalidName::code`] returns.
    pub const ALL_CODES: &[&str] = &[
        "rename_name_empty",
        "rename_name_dot",
        "rename_name_separator",
        "rename_name_reserved_char",
        "rename_name_control_char",
        "rename_name_reserved",
        "rename_name_trailing_dot_space",
        "rename_name_too_long",
        "rename_path_too_long",
        "rename_path_parent",
        "rename_path_absolute",
        "rename_path_drive",
        "rename_path_unc",
        "rename_path_empty_part",
        "rename_name_direction",
    ];

    /// The stable code; its message is `error.<code>` in the i18n catalogue.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "rename_name_empty",
            Self::DotName => "rename_name_dot",
            Self::Separator => "rename_name_separator",
            Self::ReservedChar { .. } => "rename_name_reserved_char",
            Self::ControlChar => "rename_name_control_char",
            Self::TurnsDirection => "rename_name_direction",
            Self::ReservedName => "rename_name_reserved",
            Self::TrailingDotOrSpace => "rename_name_trailing_dot_space",
            Self::NameTooLong => "rename_name_too_long",
            Self::PathTooLong => "rename_path_too_long",
            Self::ParentDir => "rename_path_parent",
            Self::AbsolutePath => "rename_path_absolute",
            Self::DrivePath => "rename_path_drive",
            Self::UncPath => "rename_path_unc",
            Self::EmptyPart => "rename_path_empty_part",
        }
    }
}

/// The longest single name on both rule sets.
const MAX_NAME: usize = 255;

/// The characters Windows refuses besides the separators and control characters.
const WINDOWS_RESERVED_CHARS: &str = "<>:\"|?*";

/// The device names Windows reserves, with any extension. `COM` and `LPT` also take the
/// superscript digits one to three.
const WINDOWS_DEVICES: &[&str] = &["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];

/// Whether `name` can be created in `dir`; the first reason it cannot. The Windows rules hold
/// on every file system; `rules` sets only how lengths are counted.
#[must_use]
pub fn check_name(name: &str, dir: &str, rules: &FsRules) -> Option<InvalidName> {
    if let Some(problem) = rule_problem(name) {
        return Some(problem);
    }
    if length(name, rules) > MAX_NAME {
        return Some(InvalidName::NameTooLong);
    }
    if path_too_long(name, dir, rules) {
        return Some(InvalidName::PathTooLong);
    }
    None
}

/// Whether `name` is one plain name every file system - Windows first - can hold; the first
/// reason it is not. The rule [`check_name`] holds a new name to, with its length counted in
/// bytes (never fewer than UTF-16 units) and no folder to fit into.
#[must_use]
pub fn check_plain(name: &str) -> Option<InvalidName> {
    rule_problem(name).or_else(|| (name.len() > MAX_NAME).then_some(InvalidName::NameTooLong))
}

/// The first rule `name` breaks, its length aside.
fn rule_problem(name: &str) -> Option<InvalidName> {
    if name.is_empty() {
        return Some(InvalidName::Empty);
    }
    if name == "." || name == ".." {
        return Some(InvalidName::DotName);
    }
    if name.contains(['/', '\\']) {
        return Some(InvalidName::Separator);
    }
    if name.chars().any(char::is_control) {
        return Some(InvalidName::ControlChar);
    }
    if name.chars().any(turns_direction) {
        return Some(InvalidName::TurnsDirection);
    }
    if let Some(ch) = name.chars().find(|c| WINDOWS_RESERVED_CHARS.contains(*c)) {
        return Some(InvalidName::ReservedChar { ch });
    }
    if is_windows_device(name) {
        return Some(InvalidName::ReservedName);
    }
    if name.ends_with(['.', ' ']) {
        return Some(InvalidName::TrailingDotOrSpace);
    }
    None
}

/// The length of `text` as `rules` count it: UTF-16 units or bytes.
fn length(text: &str, rules: &FsRules) -> usize {
    if rules.names == NameRules::Windows {
        text.encode_utf16().count()
    } else {
        text.len()
    }
}

/// Whether `dir` and `relative` joined are longer than [`FsRules::max_path`].
pub(crate) fn path_too_long(relative: &str, dir: &str, rules: &FsRules) -> bool {
    let separator = usize::from(!dir.ends_with(['/', '\\']));
    length(dir, rules) + separator + length(relative, rules) > rules.max_path
}

/// `CON`, `nul.txt`, `COM1.tar.gz`, `lpt\u{b9}`: the part before the first dot, without trailing
/// spaces, is a device name.
fn is_windows_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.to_uppercase();
    if WINDOWS_DEVICES.contains(&upper.as_str()) {
        return true;
    }
    let Some(number) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut digits = number.chars();
    matches!(
        (digits.next(), digits.next()),
        (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
