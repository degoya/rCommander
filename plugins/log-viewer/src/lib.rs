//! rCommander's bundled log file viewer (RC-100-25): levels, timestamps, addresses, paths and
//! stack traces coloured (`highlighted`), for logs of hundreds of MiB.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the batch viewer's design, with a lexer state of one record:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's state
//!   there ([`lines`]). Nothing scales with the file but the index, and the index is bounded
//!   ([`INDEX_BUDGET_BYTES`]). With `level_colour = word` the pass does not lex at all: a line's
//!   tokens then depend on that line alone.
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file, and a line in one pass from its start.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** The file is read in the encoding of the setting `encoding`
//!   (`rc-plugin-text`); `text` writes it as UTF-8, what is not valid as U+FFFD, and the tokens'
//!   offsets are UTF-8 bytes of it.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

mod decode;
#[cfg(target_arch = "wasm32")]
mod guest;
mod lexer;
pub mod lines;
mod reader;
pub mod tokens;
pub mod types;

#[cfg(test)]
mod test_support;

use rc_plugin_text::{Decoder, Encoding, detect};

use decode::{line_text, utf8_valid};
use lexer::{LineState, lex_line};
use lines::LineIndex;
use reader::{Reader, Unit};
use types::{
    Document, EncodingSetting, Failure, FailureKind, LevelColour, Output, Settings, Sink, Source,
    Token,
};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;
/// The most memory the index of one document takes. `manifest.toml`'s limits leave room for the
/// windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 8 << 20;

/// The bytes `probe` and `open` look at for a byte-order mark and, with `auto`, for UTF-8.
const HEAD_BYTES: u64 = 4096;

/// Whether the file looks like text: a UTF-16 byte-order mark or the setting `encoding` naming
/// UTF-16, else no NUL byte in its first 4 KiB (`a.out` is a program, not a log). rCommander
/// matches the manifest's extensions itself; this is `probe`.
pub fn probe<S: Source>(src: &mut S, encoding: EncodingSetting) -> bool {
    let length = u32::try_from(src.size().min(HEAD_BYTES)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    let (found, bom) = detect(&head, Encoding::Windows1252);
    let utf16 = |encoding: Encoding| matches!(encoding, Encoding::Utf16Le | Encoding::Utf16Be);
    (bom > 0 && utf16(found))
        || matches!(encoding, EncodingSetting::Fixed(chosen) if utf16(chosen))
        || !head.contains(&0)
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark of the encoding read in.
    start: u64,
    /// The encoding the file is read in, `auto` resolved.
    encoding: Encoding,
    colour: LevelColour,
    lines: LineIndex,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once, as `settings` say, and offers `highlighted`.
    pub fn open(mut src: S, settings: Settings) -> Result<(Self, Document), Failure> {
        let length = u32::try_from(src.size().min(HEAD_BYTES)).unwrap_or(4096);
        let head = if length > 0 {
            src.read_at(0, length)?
        } else {
            Vec::new()
        };
        let (found, bom) = detect(&head, Encoding::Windows1252);
        let (mut encoding, skipped) = match settings.encoding {
            EncodingSetting::Auto => (found, bom),
            // A byte-order mark of another encoding is read as text of the chosen one.
            EncodingSetting::Fixed(chosen) => (chosen, if found == chosen { bom } else { 0 }),
        };
        let start = skipped as u64;
        // `auto` without a byte-order mark: UTF-8 only if the whole file is, checked on the way.
        let check =
            settings.encoding == EncodingSetting::Auto && bom == 0 && found == Encoding::Utf8;
        let colour = settings.level_colour;
        let mut lines = LineIndex::new(start);
        let (count, valid) = {
            let mut reader = Reader::new(&mut src, start, OPEN_CHUNK, Unit::of(encoding));
            let mut raw = Vec::new();
            let mut text = String::new();
            let mut state = LineState::default();
            let mut count = 0u64;
            let mut valid = true;
            while let Some(capped) = reader.read_line(tokens::MAX_LINE_BYTES, &mut raw)? {
                if check && valid {
                    valid = utf8_valid(&raw, capped);
                }
                if colour == LevelColour::Line {
                    let line = line_text(&raw, capped, encoding, &mut text);
                    state = lex_line(line, state, colour, None);
                }
                count += 1;
                lines.line_start(count, reader.offset(), state);
            }
            (count, valid)
        };
        lines.finish(count);
        if check && !valid {
            // The lexer decided on ASCII, so the index holds for Windows-1252 as it was taken.
            encoding = Encoding::Windows1252;
        }
        let viewer = Self {
            src,
            start,
            encoding,
            colour,
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
            self.colour,
            first,
            wanted,
        )
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
