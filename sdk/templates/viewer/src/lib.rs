//! A scaffold viewer plugin for rCommander. It compiles, packages and passes conformance as it
//! is.
//!
//! It shows CSV and TSV files as a table - a Markdown table the viewer renders - and any other
//! file it is handed as text. Small enough to read in one sitting, it shows what every viewer
//! needs: reading a file in windows, naming the document's outputs, writing one into a sink.
//!
//! What the host promises a viewer, which shapes how this is written (the wiki's page "Plugin
//! development" has the whole contract):
//!
//! - **The file is a handle, never a path.** `source` gives its name, size and bytes, read in
//!   windows of at most 1 MiB; there is no file system to reach anything else.
//! - **One document, several views.** `open` names every output the document offers, the first
//!   shown, each one listed in the manifest's `[viewer] outputs`; the host refuses any other.
//!   This viewer names `markdown` for a table and `text` for anything else.
//! - **One store per document.** What `open` learned stays for the calls after it (`text`
//!   here), until the person closes the file.
//! - **Settings are read, not stored.** The manifest's `[[settings]]` appear on the plugin's
//!   page in rCommander's settings; `open` reads `separator` through the contract's `settings`
//!   and hands it in here, so the logic stays testable without a host.
//! - **Moving on cancels.** Quick View moves to the next file: ask `should-stop` between
//!   windows and end with `cancelled`.
//! - **Budgets per call.** Memory, fuel and time come from `[limits]`; reading a byte earns fuel,
//!   so a viewer that streams its file is not starved by the file's size. The table is built from
//!   the first [`TABLE_BYTES`] and at most [`TABLE_ROWS`] rows: a Markdown table is for reading,
//!   and nothing here scales with the size of the file.
//!
//! The layout follows one practical concern: everything that can be tested without a
//! WebAssembly toolchain lives here, outside the component. `cargo test` runs it on the host
//! target; `guest` - the translation into the WIT vocabulary - exists only on `wasm32`.

#[cfg(target_arch = "wasm32")]
mod guest;
pub mod table;
pub mod types;

use types::{Document, Failure, FailureKind, Output, Sink, Source};

/// The most bytes one `read-at` or `write-at` moves.
pub const WINDOW: u32 = 1 << 20;
/// The most of the file the table is built from.
pub const TABLE_BYTES: u64 = 4 << 20;
/// The most rows the table shows, its header included.
pub const TABLE_ROWS: usize = 10_000;

/// Whether the file looks like a table: its start is UTF-8 and holds a separator.
pub fn probe(src: &mut impl Source) -> bool {
    let length = u32::try_from(src.size().min(4096)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    utf8_prefix(&head, src.size() > u64::from(length))
        .is_some_and(|text| table::separator(&src.name(), text).is_some())
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The Markdown table, or `None` when the file is shown as text.
    table: Option<String>,
    /// The encoding `text` names for a file shown as text.
    encoding: &'static str,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the start of the file and offers `markdown` for a table, else `text`.
    /// `chosen` is the separator the person set ([`table::chosen_separator`]); `None` finds the
    /// file's own.
    pub fn open(mut src: S, chosen: Option<u8>) -> Result<(Self, Document), Failure> {
        let size = src.size();
        let wanted = size.min(TABLE_BYTES);
        let head = read_prefix(&mut src, wanted)?;
        let complete = wanted == size;
        let utf8 = utf8_prefix(&head, !complete);
        let name = src.name();
        let table = utf8
            .and_then(|text| {
                let separator = chosen.or_else(|| table::separator(&name, text))?;
                Some(table::rows(text, separator, complete, TABLE_ROWS))
            })
            .filter(|rows| rows.rows.iter().any(|row| row.len() > 1))
            .map(|rows| table::markdown(&rows));
        let output = if table.is_some() {
            Output::Markdown
        } else {
            Output::Text
        };
        let viewer = Self {
            src,
            table,
            encoding: if utf8.is_some() {
                "utf-8"
            } else {
                "windows-1252"
            },
        };
        let document = Document {
            outputs: vec![output],
            title: None,
        };
        Ok((viewer, document))
    }

    /// `text`: writes the table, or the file as it is, into `out` and names its encoding.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        if let Some(table) = &self.table {
            let mut offset = 0_u64;
            for window in table.as_bytes().chunks(WINDOW as usize) {
                out.write_at(offset, window)?;
                offset += window.len() as u64;
            }
            return Ok("utf-8".to_owned());
        }
        let size = self.src.size();
        let mut offset = 0_u64;
        while offset < size {
            if self.src.should_stop() {
                return Err(Failure::of(FailureKind::Cancelled));
            }
            let window = self.src.read_at(offset, WINDOW)?;
            if window.is_empty() {
                break;
            }
            out.write_at(offset, &window)?;
            offset += window.len() as u64;
        }
        Ok(self.encoding.to_owned())
    }
}

/// The first `length` bytes of `src`, read in windows.
fn read_prefix(src: &mut impl Source, length: u64) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    while (bytes.len() as u64) < length {
        if src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let left = u32::try_from(length - bytes.len() as u64).unwrap_or(WINDOW);
        let window = src.read_at(bytes.len() as u64, left.min(WINDOW))?;
        if window.is_empty() {
            break;
        }
        bytes.extend_from_slice(&window);
    }
    Ok(bytes)
}

/// `bytes` without a byte-order mark if they are UTF-8; with `cut` a character cut at the end
/// is left out.
fn utf8_prefix(bytes: &[u8], cut: bool) -> Option<&[u8]> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(_) => Some(bytes),
        Err(error) if cut && error.error_len().is_none() => Some(&bytes[..error.valid_up_to()]),
        Err(_) => None,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
