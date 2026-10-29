//! rCommander's bundled batch file viewer (RC-100-19): `.bat` and `.cmd` coloured like an editor
//! (`highlighted`), for files of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the YAML viewer's design, with a lexer state a line long:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's state
//!   there ([`lines`]). Nothing scales with the file but the index, and the index is bounded
//!   ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** The file is read in the encoding of the setting `encoding`
//!   ([`Encoding`]); `text` writes it as UTF-8 and the tokens' offsets are UTF-8 bytes of it.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

mod encoding;
#[cfg(target_arch = "wasm32")]
mod guest;
mod lexer;
pub mod lines;
mod reader;
pub mod tokens;
pub mod types;

#[cfg(test)]
mod test_support;

use encoding::Utf8Check;
use lexer::{LineState, lex_line};
use lines::LineIndex;
use rc_plugin_text::Decoder;
use reader::Reader;
use types::{Document, Encoding, Failure, FailureKind, Output, Sink, Source, Token};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;
/// The most memory the index of one document takes. `manifest.toml`'s limits leave room for the
/// windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 16 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;

/// Whether the file looks like text: no NUL byte in its first 4 KiB. A batch file has no
/// signature, and rCommander matches the manifest's extensions itself; this is `probe`.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    src.read_at(0, length).is_ok_and(|head| !head.contains(&0))
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark read as UTF-8.
    start: u64,
    /// The encoding the file is read in, `auto` resolved.
    encoding: Encoding,
    lines: LineIndex,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once, in the encoding `setting` names, and offers `highlighted`.
    pub fn open(mut src: S, setting: Encoding) -> Result<(Self, Document), Failure> {
        let bom = src.size() >= 3 && src.read_at(0, 3)? == BOM;
        let utf8 = matches!(setting, Encoding::Auto | Encoding::Utf8);
        let start = if bom && utf8 { 3 } else { 0 };
        let check = (setting == Encoding::Auto && !bom).then(Utf8Check::default);
        let mut lines = LineIndex::new(start);
        let (count, valid) = {
            let mut reader = Reader::new(&mut src, start, OPEN_CHUNK);
            if let Some(check) = check {
                reader = reader.with_check(check);
            }
            let mut line = Vec::new();
            let mut state = LineState::default();
            let mut count = 0u64;
            // The lexer reads the file's bytes, not their text: it decides on ASCII alone, so the
            // state is the same, and `auto` is known only at the end (`encoding.rs`).
            while reader
                .read_line(tokens::MAX_LINE_BYTES, &mut line)?
                .is_some()
            {
                state = lex_line(&line, state, None);
                count += 1;
                lines.line_start(count, reader.offset(), state);
            }
            (count, reader.utf8_valid())
        };
        lines.finish(count);
        let encoding = match setting {
            Encoding::Auto if bom || valid => Encoding::Utf8,
            Encoding::Auto => Encoding::Cp850,
            chosen => chosen,
        };
        let viewer = Self {
            src,
            start,
            encoding,
            lines,
        };
        let document = viewer.document();
        Ok((viewer, document))
    }

    /// The outputs this document offers: `highlighted` alone.
    #[must_use]
    pub fn document(&self) -> Document {
        Document {
            outputs: vec![Output::Highlighted],
            title: None,
            pages: None,
        }
    }

    /// The number of lines of the text.
    #[must_use]
    pub fn line_count(&self) -> u64 {
        self.lines.lines()
    }

    /// The encoding the file is read in: the setting's, `auto` resolved.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes()
    }

    /// `text`: writes the file from after a byte-order mark read as UTF-8 into `out`, as UTF-8.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let size = self.src.size();
        let mut decoder = Decoder::new(self.encoding.text());
        let mut text = String::new();
        let mut at = self.start;
        let mut written = 0u64;
        loop {
            text.clear();
            let window = u64::from(OPEN_CHUNK).min(size.saturating_sub(at));
            if window > 0 {
                if self.src.should_stop() {
                    return Err(Failure::of(FailureKind::Cancelled));
                }
                let length = u32::try_from(window).unwrap_or(OPEN_CHUNK);
                let mut bytes = self.src.read_at(at, length)?;
                bytes.truncate(length as usize);
                at += bytes.len() as u64;
                decoder.push(&bytes, &mut text);
                if bytes.is_empty() {
                    // A source shorter than announced ends here rather than looping.
                    at = size;
                }
            }
            let last = at >= size;
            if last {
                decoder.finish(&mut text);
            }
            for piece in text.as_bytes().chunks(WRITE_CHUNK) {
                out.write_at(written, piece)?;
                written += piece.len() as u64;
            }
            if last {
                return Ok("utf-8".to_owned());
            }
        }
    }

    /// `tokens`: the tokens of lines `first_line..first_line + count`, one list per line that
    /// exists.
    pub fn tokens(&mut self, first_line: u32, count: u32) -> Result<Vec<Vec<Token>>, Failure> {
        let first = u64::from(first_line);
        let wanted = u64::from(count).min(self.lines.lines().saturating_sub(first));
        if wanted == 0 {
            return Ok(Vec::new());
        }
        tokens::line_tokens(&mut self.src, &self.lines, self.encoding, first, wanted)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
