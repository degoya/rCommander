//! Reading an export of Devolutions Remote Desktop Manager (RC-050-11): a `.rdm` file written
//! without a master key, which is XML. Nothing here touches the connection tree.
//!
//! - The file is foreign input: at most [`MAX_FILE_BYTES`], parsed by `roxmltree` with DTDs
//!   refused (so no entity is declared, let alone expanded or fetched) and at most [`MAX_NODES`]
//!   nodes; at most [`MAX_ENTRIES`] entries are taken.
//! - An export protected with a master key is refused ([`RdmError::Encrypted`]): text that is no
//!   XML, an encryption flag at the top, or an `RDMExport` that holds something but no
//!   `Connections`. RDM documents no format, so this is what the owner's 2026.3 export and the
//!   documented behaviour ("the master key encrypts the whole content") allow to tell.
//! - An entry is a `Connection`: its `ID`, `Name`, `ConnectionType` and `Group` (folders joined
//!   by `\`) at the top, its address in the element of its type (`Terminal` for `SSHShell`),
//!   names compared ignoring ASCII case. Stage 1 takes SSH only (owner, 2026-10-18): SSH shells,
//!   PuTTY's SSH, SFTP and SCP become SSH connections; FTP and FTPS are shown as not supported
//!   yet - they follow once an export with FTP entries is at hand, as one arm of `kind_of` -
//!   and anything else as not supported. Folder entries (`Group`) are no row.
//! - Passwords are never read: `SafePassword` is RDM's own obfuscation (undocumented), and only
//!   whether one is there is kept ([`RdmPassword`]). The password history is not looked at.
//! - A key file stays a path, taken only when it is a plain local one ([`key_path_allowed`]):
//!   never a share (`\\server`, which Windows would log on to) or a device path.

use std::{collections::HashSet, io, path::Path};

use roxmltree::{Document, Error as XmlError, Node, ParsingOptions};
use serde::{Deserialize, Serialize};

use super::bounded_read::{BoundedReadError, read_bounded};
use crate::ConnectionKind;

/// The largest file read.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The most XML nodes parsed.
pub const MAX_NODES: u32 = 2_000_000;

/// The most entries taken; the rest is left out with a warning.
pub const MAX_ENTRIES: usize = 4096;

/// How deep an entry's folders nest below the import's folder.
const MAX_FOLDERS: usize = 8;

/// The longest name, folder, host, user or key path taken, in characters.
const MAX_VALUE: usize = 255;

/// The longest RDM type name repeated in a report.
const MAX_TYPE: usize = 40;

/// Why the file is not read.
#[derive(Debug)]
pub enum RdmError {
    TooLarge,
    NotAFile,
    Io(io::Error),
    /// Protected with a master key, or no XML at all.
    Encrypted,
    /// Broken XML, or XML with a DTD; the line of the first fault.
    Invalid {
        line: u32,
    },
    /// XML, but not an export of Remote Desktop Manager.
    NotAnExport,
}

/// Why an entry is not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RdmSkip {
    /// A type rCommander has no connection for (RDP, VNC, web, ...).
    NotSupported,
    /// FTP or FTPS: not imported in stage 1, to follow (owner, 2026-10-18).
    NotSupportedYet,
    /// No usable `ID`, so a second import could not find it again.
    NoId,
    /// An `ID` an earlier entry of the file has.
    DuplicateId,
    /// Nested in more folders than the import takes.
    TooDeep,
    /// A folder name with a control character.
    FolderInvalid,
    /// A name, folder, host or user longer than the import takes.
    ValueTooLong,
}

impl RdmSkip {
    /// Every value, for the window's texts.
    pub const ALL: &[Self] = &[
        Self::NotSupported,
        Self::NotSupportedYet,
        Self::NoId,
        Self::DuplicateId,
        Self::TooDeep,
        Self::FolderInvalid,
        Self::ValueTooLong,
    ];

    /// The `refused` code of the report; the window's text is `error.<code>`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::NotSupported => "rdm_not_supported",
            Self::NotSupportedYet => "rdm_not_supported_yet",
            Self::NoId => "rdm_no_id",
            Self::DuplicateId => "rdm_duplicate_id",
            Self::TooDeep => "rdm_too_deep",
            Self::FolderInvalid => "rdm_folder_invalid",
            Self::ValueTooLong => "rdm_value_too_long",
        }
    }
}

/// Whether RDM keeps a password for an entry: only ever this, never the value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RdmPassword {
    #[default]
    None,
    /// A `SafePassword`: obfuscated by RDM, not taken (stage 2 is No-Go).
    Obfuscated,
}

/// One entry of the export. Holds no secret: a password only as [`RdmPassword`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdmEntry {
    /// The `ID` in ASCII lower case; `None` when missing or not one.
    pub id: Option<String>,
    pub name: String,
    /// The folders of `Group`, outermost first.
    pub folders: Vec<String>,
    /// `ConnectionType` as RDM writes it, cut to a plain word.
    pub rdm_type: String,
    pub kind: Result<ConnectionKind, RdmSkip>,
    pub host: String,
    /// Wider than `u16`, so a bad port reaches the validation's `port_invalid`.
    pub port: Option<i64>,
    pub user: Option<String>,
    pub key_path: Option<String>,
    /// A key path was there but is no plain local path, and was left out.
    pub key_path_refused: bool,
    pub password: RdmPassword,
}

/// What the file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RdmExport {
    pub entries: Vec<RdmEntry>,
    /// More than [`MAX_ENTRIES`] entries: the rest was left out.
    pub truncated: bool,
}

/// Reads and parses the export at `path`.
///
/// # Errors
///
/// Not a regular file, unreadable, larger than [`MAX_FILE_BYTES`], or [`parse`]'s errors.
pub fn read_file(path: &Path) -> Result<RdmExport, RdmError> {
    let bytes = read_bounded(path, MAX_FILE_BYTES).map_err(|error| match error {
        BoundedReadError::Io(error) => RdmError::Io(error),
        BoundedReadError::NotAFile => RdmError::NotAFile,
        BoundedReadError::TooLarge { .. } => RdmError::TooLarge,
    })?;
    parse(&bytes)
}

/// Parses an export.
///
/// # Errors
///
/// [`RdmError::Encrypted`], [`RdmError::Invalid`] or [`RdmError::NotAnExport`].
pub fn parse(bytes: &[u8]) -> Result<RdmExport, RdmError> {
    let text = decode(bytes).ok_or(RdmError::Encrypted)?;
    if !text.trim_start().starts_with('<') {
        return Err(RdmError::Encrypted);
    }
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_NODES,
    };
    let document = Document::parse_with_options(&text, options).map_err(|error| {
        let line = match error {
            XmlError::DtdDetected | XmlError::NodesLimitReached => 1,
            other => other.pos().row,
        };
        RdmError::Invalid { line }
    })?;
    let root = document.root_element();
    let list = if is(root, "RDMExport") {
        if encrypted_flag(root) {
            return Err(RdmError::Encrypted);
        }
        match child(root, "Connections") {
            Some(list) => list,
            // An empty export has its version and database id only.
            None if elements(root).all(|node| is(node, "Version") || is(node, "DatabaseID")) => {
                return Ok(RdmExport::default());
            }
            None => return Err(RdmError::Encrypted),
        }
    } else if is(root, "ArrayOfConnection") {
        // What RDM's copy and older exports write: the entries without the export around them.
        root
    } else {
        return Err(RdmError::NotAnExport);
    };
    let mut export = RdmExport::default();
    for node in elements(list).filter(|node| is(*node, "Connection")) {
        if export.entries.len() == MAX_ENTRIES {
            export.truncated = true;
            break;
        }
        if let Some(found) = entry(node) {
            export.entries.push(found);
        }
    }
    let mut seen: HashSet<&str> = HashSet::new();
    let mut duplicates = Vec::new();
    for (at, found) in export.entries.iter().enumerate() {
        if let Some(id) = &found.id
            && !seen.insert(id.as_str())
        {
            duplicates.push(at);
        }
    }
    for at in duplicates {
        export.entries[at].kind = Err(RdmSkip::DuplicateId);
    }
    Ok(export)
}

/// The text of `bytes`: UTF-8, or UTF-8 or UTF-16 after a byte order mark; `None` otherwise.
///
/// Deliberately not the viewer's `decode_text` (RC-080-21): an export is XML, which Remote
/// Desktop Manager writes in Unicode only, and an encrypted export is binary. Guessing
/// Windows-1252 or UTF-16 without a mark would turn ciphertext into text; a byte that does not
/// decode means [`RdmError::Encrypted`].
fn decode(bytes: &[u8]) -> Option<String> {
    if let Some((encoding, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        let (text, errors) = encoding.decode_without_bom_handling(&bytes[bom..]);
        return (!errors).then(|| text.into_owned());
    }
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

fn is(node: Node<'_, '_>, name: &str) -> bool {
    node.tag_name().name().eq_ignore_ascii_case(name)
}

fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// The first child element of `node` named `name`.
fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    elements(node).find(|child| is(*child, name))
}

/// The trimmed text of the first child element of `node` named one of `names`, when not empty.
fn value(node: Node<'_, '_>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .filter_map(|name| child(node, name))
        .filter_map(|child| child.text())
        .map(str::trim)
        .find(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Whether the export says at its top that it is encrypted (an element named for it whose value
/// is not `false`).
fn encrypted_flag(root: Node<'_, '_>) -> bool {
    elements(root).any(|node| {
        let name = node.tag_name().name().to_ascii_lowercase();
        let text = node.text().unwrap_or_default().trim().to_ascii_lowercase();
        name.contains("encrypt") && !matches!(text.as_str(), "" | "false" | "0")
    })
}

/// The connection an RDM type becomes, or why it becomes none. Stage 1 takes SSH only (owner,
/// 2026-10-18); FTP and FTPS follow here once an export with FTP entries is at hand.
fn kind_of(rdm_type: &str) -> Result<ConnectionKind, RdmSkip> {
    match rdm_type.to_ascii_lowercase().as_str() {
        "sshshell" | "putty" | "sftp" | "scp" => Ok(ConnectionKind::Ssh),
        "ftp" | "ftps" => Err(RdmSkip::NotSupportedYet),
        _ => Err(RdmSkip::NotSupported),
    }
}

/// The element that holds an entry's address: the one named for its type, `Terminal`, or the
/// first that has a `Host`.
fn settings<'a, 'input>(node: Node<'a, 'input>, rdm_type: &str) -> Option<Node<'a, 'input>> {
    let named = (!rdm_type.is_empty())
        .then(|| child(node, rdm_type))
        .flatten();
    named
        .or_else(|| child(node, "Terminal"))
        .or_else(|| elements(node).find(|child| value(*child, &["Host"]).is_some()))
}

/// One entry; `None` for a folder.
fn entry(node: Node<'_, '_>) -> Option<RdmEntry> {
    let raw_type = value(node, &["ConnectionType"]).unwrap_or_default();
    if raw_type.eq_ignore_ascii_case("Group") {
        return None;
    }
    let rdm_type: String = raw_type
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_TYPE)
        .collect();
    let place = settings(node, &rdm_type);
    let found = |names: &[&str]| place.and_then(|place| value(place, names));
    let host = found(&["Host"])
        .or_else(|| value(node, &["Url"]))
        .unwrap_or_default();
    let port = found(&["HostPort", "Port"]).and_then(|port| match port.parse::<i64>() {
        Ok(0) => None,
        Ok(port) => Some(port),
        Err(_) => Some(-1),
    });
    let user = found(&["Username", "UserName", "LoginName"])
        .or_else(|| value(node, &["UserName", "Username"]));
    let raw_key = found(&["PrivateKeyPath", "PrivateKeyFile"]);
    let key_path = raw_key.clone().filter(|path| key_path_allowed(path));
    let has_password = [Some(node), place]
        .into_iter()
        .flatten()
        .any(|at| value(at, &["SafePassword"]).is_some());
    let folders: Vec<String> = value(node, &["Group"])
        .unwrap_or_default()
        .split('\\')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    let name = value(node, &["Name"]).unwrap_or_default();
    let id = value(node, &["ID"])
        .map(|id| id.to_ascii_lowercase())
        .filter(|id| {
            id.len() <= 64
                && id
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || matches!(c, '-' | '{' | '}'))
        });
    let long = [Some(&name), Some(&host), user.as_ref()]
        .into_iter()
        .flatten()
        .chain(&folders)
        .any(|text| text.chars().count() > MAX_VALUE);
    let kind = match kind_of(&rdm_type) {
        Err(skip) => Err(skip),
        Ok(_)
            if rdm_type.eq_ignore_ascii_case("putty")
                && found(&["ProtocolType"])
                    .is_some_and(|kind| !kind.eq_ignore_ascii_case("ssh")) =>
        {
            Err(RdmSkip::NotSupported)
        }
        _ if id.is_none() => Err(RdmSkip::NoId),
        _ if folders
            .iter()
            .any(|name| name.chars().any(char::is_control)) =>
        {
            Err(RdmSkip::FolderInvalid)
        }
        _ if folders.len() > MAX_FOLDERS => Err(RdmSkip::TooDeep),
        _ if long => Err(RdmSkip::ValueTooLong),
        Ok(kind) => Ok(kind),
    };
    Some(RdmEntry {
        id,
        name: cut(name),
        folders: folders.into_iter().map(cut).collect(),
        rdm_type,
        kind,
        host: cut(host),
        port,
        user: user.map(cut),
        key_path_refused: raw_key.is_some() && key_path.is_none(),
        key_path: key_path.map(cut),
        password: if has_password {
            RdmPassword::Obfuscated
        } else {
            RdmPassword::None
        },
    })
}

/// `text` cut to [`MAX_VALUE`] characters, for the report.
fn cut(text: String) -> String {
    if text.chars().count() > MAX_VALUE {
        text.chars().take(MAX_VALUE).collect()
    } else {
        text
    }
}

/// Whether `path` is a plain local key path: a drive path (`C:\`), an absolute Unix path or one
/// below `~`, without `..` or a control character. A share (`\\server\...`, `//server/...`), a
/// device path (`\\?\`, `\\.\`), a relative path and RDM's `$VARIABLES$` are not.
#[must_use]
pub fn key_path_allowed(path: &str) -> bool {
    let drive = rc_vfs::drive_letter(path).is_some()
        && matches!(path.as_bytes().get(2), Some(b'\\' | b'/'));
    let unix = path.starts_with('/') && !path.starts_with("//");
    let home = path.starts_with("~/") || path.starts_with("~\\");
    (drive || unix || home)
        && path.chars().count() <= MAX_VALUE
        && !path.chars().any(|c| c.is_control() || c == '$')
        && !path.split(['/', '\\']).any(|part| part == "..")
}

#[cfg(test)]
#[path = "rdm_tests.rs"]
mod tests;
