//! rCommander's bundled PowerShell viewer (RC-100-23): `.ps1`, `.psm1` and `.psd1` coloured like
//! an editor (`highlighted`), for files of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the batch viewer's design, with the frames of the shell viewer:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's state
//!   there ([`lines`]) - open strings, here-strings, block comments, `$( )` and a backtick
//!   continuation. Nothing scales with the file but the index, and the index is bounded
//!   ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** The file is read in the encoding of the setting `encoding`, decoded
//!   by `rc-plugin-text`; `text` writes it as UTF-8, and the lexer reads the same text a line at
//!   a time, so the tokens' offsets are UTF-8 bytes of what the Lister shows.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

#[cfg(target_arch = "wasm32")]
mod guest;
mod lexer;
pub mod lines;
mod reader;
pub mod tokens;
pub mod types;

#[cfg(test)]
mod test_support;

pub use rc_plugin_text::Encoding;
use rc_plugin_text::{Decoder, detect};

use lexer::{LineState, Opens, QuoteOpen, lex_line};
use lines::LineIndex;
use reader::Reader;
use types::{Document, Failure, FailureKind, Output, Sink, Source, Token};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;
/// The most memory the index of one document takes. `manifest.toml`'s limits leave room for the
/// windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 16 << 20;
/// What `auto` reads when the file has no byte-order mark and is not UTF-8: Windows PowerShell
/// 5.1 reads and saves a script without one in the ANSI code page.
const AUTO_FALLBACK: Encoding = Encoding::Windows1252;

/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;

/// The setting `encoding`: `None` for `auto` - and for anything the manifest does not offer.
#[must_use]
pub fn encoding_setting(value: Option<&str>) -> Option<Encoding> {
    value.filter(|value| *value != "auto")?.parse().ok()
}

/// Whether the file looks like text: no NUL byte in its first 4 KiB, or NUL bytes on one side of
/// each pair only, as UTF-16 text has them. A script has no signature, and rCommander matches
/// the manifest's extensions itself; this is `probe`.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    let mut nul = head
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == 0)
        .map(|(at, _)| at % 2);
    match nul.next() {
        None => true,
        Some(side) => nul.all(|other| other == side),
    }
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark of the encoding read.
    start: u64,
    /// The encoding the file is read in, `auto` resolved.
    encoding: Encoding,
    lines: LineIndex,
    /// The strings the file leaves open at its end: their opening shows as `invalid`.
    unclosed: Vec<QuoteOpen>,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once, in the encoding `setting` names (`None`: `auto`), and offers
    /// `highlighted`.
    pub fn open(mut src: S, setting: Option<Encoding>) -> Result<(Self, Document), Failure> {
        if src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let size = src.size();
        let length = u32::try_from(size.min(u64::from(OPEN_CHUNK))).unwrap_or(OPEN_CHUNK);
        let mut head = src.read_at(0, length)?;
        head.truncate(length as usize);
        src.progress(head.len() as u64, Some(size));
        let (encoding, bom) = match setting {
            None => detect(&head, AUTO_FALLBACK),
            // A byte-order mark is skipped only when it is the chosen encoding's own.
            Some(chosen) => match detect(&head, chosen) {
                (found, bom) if found == chosen => (chosen, bom),
                _ => (chosen, 0),
            },
        };
        let start = bom as u64;
        let mut lines = LineIndex::new(start);
        let mut opens = Opens::default();
        {
            let mut reader =
                Reader::new(&mut src, start, OPEN_CHUNK, encoding).with_window(0, head);
            let mut bytes = Vec::new();
            let mut text = String::new();
            let mut state = LineState::default();
            while let Some(capped) = reader.read_line(tokens::MAX_LINE_BYTES, &mut bytes)? {
                text.clear();
                tokens::decode_line(&bytes, capped, encoding, &mut text);
                state = lex_line(text.as_bytes(), state, &mut opens);
                opens.line += 1;
                lines.line_start(opens.line, reader.offset(), state);
            }
        }
        lines.finish(opens.line);
        let viewer = Self {
            src,
            start,
            encoding,
            lines,
            unclosed: opens.into_unclosed(),
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
        self.lines.memory_bytes() + self.unclosed.capacity() * size_of::<QuoteOpen>()
    }

    /// `text`: writes the file from after its byte-order mark into `out`, as UTF-8.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let size = self.src.size();
        let mut decoder = Decoder::new(self.encoding);
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
        tokens::line_tokens(
            &mut self.src,
            &self.lines,
            self.encoding,
            &self.unclosed,
            first,
            wanted,
        )
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
