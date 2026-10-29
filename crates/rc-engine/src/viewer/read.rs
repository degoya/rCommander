//! What `files.read_text` and `files.read_hex` take and answer: a capped window of a file, for
//! an agent that reads a file the way the viewer shows it.

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use super::TextEncoding;
use crate::PanelId;

/// Bytes `files.read_text` reads when the call names no limit.
pub const TEXT_DEFAULT_BYTES: u64 = 256 * 1024;
/// The most bytes `files.read_text` reads in one call.
pub const TEXT_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Bytes `files.read_hex` reads when the call names no length.
pub const HEX_DEFAULT_BYTES: u64 = 4 * 1024;
/// The most bytes `files.read_hex` reads in one call.
pub const HEX_MAX_BYTES: u64 = 64 * 1024;

/// `files.read_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadTextRequest {
    /// Absolute, relative to the panel's directory, or a server's location (`sftp://`,
    /// `ftp://`).
    pub path: String,
    /// The panel a relative path starts from; the active one when `None`.
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The byte to start at; a character it cuts is skipped.
    #[serde(default)]
    pub offset: u64,
    /// Bytes of the file to read: [`TEXT_DEFAULT_BYTES`] when `None`, at most
    /// [`TEXT_MAX_BYTES`].
    #[serde(default)]
    pub max_bytes: Option<u64>,
    /// Detected from the file's head when `None`.
    #[serde(default)]
    pub encoding: Option<TextEncoding>,
}

/// `files.read_hex`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadHexRequest {
    /// Absolute, relative to the panel's directory, or a server's location (`sftp://`,
    /// `ftp://`).
    pub path: String,
    /// The panel a relative path starts from; the active one when `None`.
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The first byte of the first row.
    #[serde(default)]
    pub offset: u64,
    /// Bytes to show: [`HEX_DEFAULT_BYTES`] when `None`, at most [`HEX_MAX_BYTES`].
    #[serde(default)]
    pub length: Option<u64>,
}

/// A window of a file as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextWindow {
    pub path: VfsPath,
    /// The file's size in bytes.
    pub size: u64,
    pub encoding: TextEncoding,
    /// Whether the file starts with a byte-order mark (never part of the text).
    pub bom: bool,
    /// The bytes the text was decoded from: `start..end`. The next window starts at `end`.
    pub start: u64,
    pub end: u64,
    /// Whether the file goes on after `end`: the limit cut the text.
    pub truncated: bool,
    /// Whether the file looks like binary data (a zero byte in its head).
    pub binary: bool,
    /// Whether private key material was replaced by `[private key redacted]`.
    pub masked: bool,
    pub text: String,
}

/// A window of a file as hex rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HexWindow {
    pub path: VfsPath,
    /// The file's size in bytes.
    pub size: u64,
    /// The bytes shown: `start..end`. The next window starts at `end`.
    pub start: u64,
    pub end: u64,
    /// Whether the file goes on after `end`.
    pub truncated: bool,
    /// Whether the rows were withheld because they hold private key material.
    pub masked: bool,
    /// The rows as `xxd` prints them: offset, 16 bytes in groups of two, the characters.
    pub rows: Vec<String>,
}
