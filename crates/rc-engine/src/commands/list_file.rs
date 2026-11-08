//! The text files of the panels' lists (RC-100-92): what `panel.selection_from_file`,
//! `panel.selection_from_text` and `panel.load_list` read, and where `panel.selection_to_file`
//! writes. Only files on this machine's disks: a relative path is the panel's folder's when the
//! panel shows this machine, any other place is refused.
//!
//! A file is read whole, at most [`MAX_LIST_BYTES`] and [`MAX_LIST_LINES`] lines, through the one
//! bounded reader (`import::bounded_read`): UTF-8 or UTF-16 by its byte-order mark, else UTF-8
//! when valid, else Windows-1252 (an ANSI file Total Commander wrote), as the multi-rename's name
//! lists are read. A line is a name or a path; one in double quotes keeps its spaces.

use std::path::PathBuf;

use rc_plugin_text::{Decoder, Encoding, detect};
use rc_vfs::BackendId;

use crate::{
    Engine, EngineError, PanelId, TabsError, commands::resolve_path, flat::MAX_FLAT_ENTRIES,
    recover::LogPanic as _,
};

/// The most bytes a selection or list file (or a text) may have: 16 MiB.
pub const MAX_LIST_BYTES: u64 = 16 * 1024 * 1024;

/// The most lines a selection or list file may have: a flat view's most entries.
pub const MAX_LIST_LINES: usize = MAX_FLAT_ENTRIES;

fn too_large() -> EngineError {
    TabsError::ListFileTooLarge {
        max_bytes: MAX_LIST_BYTES,
        max_lines: MAX_LIST_LINES,
    }
    .into()
}

/// The lines of `text` as names or paths: trimmed, double quotes round one removed, blank ones
/// dropped.
///
/// # Errors
///
/// [`TabsError::ListFileTooLarge`] past [`MAX_LIST_BYTES`] or [`MAX_LIST_LINES`].
pub(crate) fn list_lines(text: &str) -> Result<Vec<String>, EngineError> {
    if text.len() as u64 > MAX_LIST_BYTES {
        return Err(too_large());
    }
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let line = line
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(line)
            .trim();
        if line.is_empty() || line.chars().any(char::is_control) {
            continue;
        }
        if lines.len() == MAX_LIST_LINES {
            return Err(too_large());
        }
        lines.push(line.to_owned());
    }
    Ok(lines)
}

/// `bytes` as text: by a byte-order mark, else UTF-8 when valid, else Windows-1252.
pub(crate) fn decoded(bytes: &[u8]) -> String {
    let (encoding, bom) = detect(bytes, Encoding::Windows1252);
    let mut text = String::with_capacity(bytes.len());
    let mut decoder = Decoder::new(encoding);
    decoder.push(&bytes[bom..], &mut text);
    decoder.finish(&mut text);
    text
}

impl Engine {
    /// `path` as a file on this machine: resolved against panel `id`'s folder.
    ///
    /// # Errors
    ///
    /// [`TabsError::ListFileNotLocal`] for a place elsewhere (a server, an archive, a device).
    pub(crate) fn local_list_file(&self, id: PanelId, path: &str) -> Result<PathBuf, EngineError> {
        let current = self.state().panel(id).path.clone();
        let target = resolve_path(&current, path.trim());
        if target.backend != BackendId::Local || !target.path.is_absolute() {
            return Err(TabsError::ListFileNotLocal.into());
        }
        Ok(target.path)
    }

    /// The lines of the list file `path` of panel `id` ([`list_lines`]), read off the async
    /// threads.
    ///
    /// # Errors
    ///
    /// The place is not on this machine, the file cannot be read, or it is too large.
    pub(crate) async fn read_list_file(
        &self,
        id: PanelId,
        path: &str,
    ) -> Result<Vec<String>, EngineError> {
        use crate::import::bounded_read::{BoundedReadError, read_bounded};

        let file = self.local_list_file(id, path)?;
        let read = {
            let file = file.clone();
            tokio::task::spawn_blocking(move || read_bounded(&file, MAX_LIST_BYTES))
                .await
                .log_panic("engine.list_file_read")
        };
        let bytes = match read {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(BoundedReadError::TooLarge { .. })) => return Err(too_large()),
            Ok(Err(error)) => return Err(rc_vfs::VfsError::from_io(error.into(), file).into()),
            Err(error) => {
                return Err(rc_vfs::VfsError::from_io(
                    std::io::Error::other(error.to_string()),
                    file,
                )
                .into());
            }
        };
        list_lines(&decoded(&bytes))
    }
}

#[cfg(test)]
#[path = "list_file_tests.rs"]
mod tests;
