//! The list files of an own command's parameters (RC-100-86): `%L` and `%F` in the system's ANSI
//! code page, `%UL` and `%UF` in UTF-8, `%WL` and `%WF` in UTF-16 (little endian, with its byte
//! order mark) - `L` the marked entries with their folder, `F` their names, one per line.
//!
//! Each start writes its lists into a folder of its own under the user's own root
//! ([`crate::unpacked::new_list_folder`], mode 0700 elsewhere than Windows), named so nobody
//! guesses it first. The lists are not removed when the program ends: a program that hands them
//! to an instance already running (Notepad on Windows 11, many editors) returns at once, and the
//! list would be gone before it was read. The app removes them at exit, or at the next start
//! once its process has ended, with the unpacked copies (`unpacked.rs`). A name the ANSI code page
//! cannot hold refuses the start with [`UserCommandError::ListEncoding`], which names `%UL`.

#[cfg(windows)]
mod windows;

use std::path::Path;

use encoding_rs::Encoding;

use super::UserCommandError;
use crate::{EngineError, recover::LogPanic as _};

/// One kind of list file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    /// `%L`: folder and name, ANSI.
    PathsAnsi,
    /// `%F`: names, ANSI.
    NamesAnsi,
    /// `%UL`: folder and name, UTF-8.
    PathsUtf8,
    /// `%UF`: names, UTF-8.
    NamesUtf8,
    /// `%WL`: folder and name, UTF-16.
    PathsUtf16,
    /// `%WF`: names, UTF-16.
    NamesUtf16,
}

impl ListKind {
    /// Whether its lines hold the folder too.
    #[must_use]
    pub fn with_folder(self) -> bool {
        matches!(self, Self::PathsAnsi | Self::PathsUtf8 | Self::PathsUtf16)
    }

    /// The file name inside the start's folder.
    fn file_name(self) -> &'static str {
        match self {
            Self::PathsAnsi => "list-ansi.txt",
            Self::NamesAnsi => "names-ansi.txt",
            Self::PathsUtf8 => "list-utf8.txt",
            Self::NamesUtf8 => "names-utf8.txt",
            Self::PathsUtf16 => "list-utf16.txt",
            Self::NamesUtf16 => "names-utf16.txt",
        }
    }
}

/// The line break of this system's text files.
const LINE_END: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// The encoding of the system's ANSI code page: the one Windows names, UTF-8 elsewhere (and for a
/// code page `encoding_rs` does not have).
#[must_use]
pub fn ansi_encoding() -> &'static Encoding {
    #[cfg(windows)]
    {
        windows::ansi_code_page()
            .and_then(encoding_of)
            .unwrap_or(encoding_rs::UTF_8)
    }
    #[cfg(not(windows))]
    {
        encoding_rs::UTF_8
    }
}

/// The encoding of the Windows code page `page`; `None` for one `encoding_rs` does not have.
#[cfg(any(windows, test))]
#[must_use]
pub fn encoding_of(page: u16) -> Option<&'static Encoding> {
    let label = match page {
        65001 => "utf-8",
        932 => "shift_jis",
        936 => "gbk",
        949 => "euc-kr",
        950 => "big5",
        874 | 1250..=1258 => return Encoding::for_label(format!("windows-{page}").as_bytes()),
        _ => return None,
    };
    Encoding::for_label(label.as_bytes())
}

/// The bytes of a list of `lines` of `kind`, `ansi` the encoding of `%L` and `%F`.
///
/// # Errors
///
/// [`UserCommandError::ListEncoding`]: a line the ANSI code page cannot hold.
pub fn encode(
    kind: ListKind,
    lines: &[String],
    ansi: &'static Encoding,
) -> Result<Vec<u8>, UserCommandError> {
    let mut text = String::new();
    for line in lines {
        text.push_str(line);
        text.push_str(LINE_END);
    }
    match kind {
        ListKind::PathsAnsi | ListKind::NamesAnsi => {
            let (bytes, _, unmappable) = ansi.encode(&text);
            if unmappable {
                return Err(UserCommandError::ListEncoding);
            }
            Ok(bytes.into_owned())
        }
        ListKind::PathsUtf8 | ListKind::NamesUtf8 => Ok(text.into_bytes()),
        ListKind::PathsUtf16 | ListKind::NamesUtf16 => {
            let mut bytes = vec![0xFF, 0xFE];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(bytes)
        }
    }
}

/// The lines of a list of `kind`: each name, with `folder` (ending in its separator) in front
/// when the kind holds folders.
#[must_use]
pub fn lines_of(kind: ListKind, folder: &str, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|name| {
            if kind.with_folder() {
                format!("{folder}{name}")
            } else {
                name.clone()
            }
        })
        .collect()
}

/// Writes the lists `kinds` of `names` (in `folder`, ending in its separator) into a new folder
/// of the user's own; each kind with the path of its file. Nothing is written for no kind.
///
/// # Errors
///
/// A name the ANSI code page cannot hold, or the folder or a file cannot be written.
pub(crate) async fn write_lists(
    kinds: &[ListKind],
    folder: &str,
    names: &[String],
) -> Result<Vec<(ListKind, String)>, EngineError> {
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    let ansi = ansi_encoding();
    let mut files = Vec::new();
    for kind in kinds {
        files.push((*kind, encode(*kind, &lines_of(*kind, folder, names), ansi)?));
    }
    let dir = crate::unpacked::new_list_folder()
        .await
        .map_err(|source| UserCommandError::Failed { source })?;
    tokio::task::spawn_blocking(move || write_files(&dir, files))
        .await
        .log_panic("engine.list_files")
        .map_err(|error| UserCommandError::Failed {
            source: std::io::Error::other(error),
        })?
        .map_err(|source| UserCommandError::Failed { source }.into())
}

/// Writes `files` into `dir`; each kind with its path. Blocking.
fn write_files(
    dir: &Path,
    files: Vec<(ListKind, Vec<u8>)>,
) -> std::io::Result<Vec<(ListKind, String)>> {
    let mut written = Vec::new();
    for (kind, bytes) in files {
        let path = dir.join(kind.file_name());
        std::fs::write(&path, bytes)?;
        written.push((kind, path.to_string_lossy().into_owned()));
    }
    Ok(written)
}

#[cfg(test)]
#[path = "list_files_tests.rs"]
mod tests;
