//! rCommander's bundled web code viewer (RC-100-26): HTML, CSS, SCSS, LESS, JavaScript,
//! TypeScript, JSX, TSX, PHP and Vue coloured like an editor (`highlighted`), for files of any
//! size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the shell viewer's design, with the nesting of the languages in the lexer's state:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's
//!   state there ([`lines`]) - the stack of languages a line starts inside of, and where each
//!   goes on: a comment, a string, a tag's attributes, a declaration's value, a PHP heredoc.
//!   Nothing scales with the file but the index, and the index is bounded
//!   ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** A byte that is not is written as U+FFFD, and token offsets count
//!   the text written, so they always fall on its character boundaries.
//!
//! The language is the file's extension's ([`Language`]); the text is the file's own, so the
//! plugin has no setting.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

#[cfg(target_arch = "wasm32")]
mod guest;
mod language;
mod lexer;
pub mod lines;
mod reader;
pub mod tokens;
pub mod types;
mod utf8;

#[cfg(test)]
mod test_support;

pub use language::Language;
use lexer::{Lexer, LineState};
use lines::LineIndex;
use reader::Reader;
use types::{Document, Failure, Output, Sink, Source, Token};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index of one document takes. `manifest.toml`'s limits leave room for the
/// windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 8 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The bytes `probe` looks at, and `open` for the language of a file without an extension.
const PROBE_BYTES: u64 = 4096;

/// Whether the file is text: no NUL byte in its first 4 KiB. rCommander offers the file by the
/// manifest's extensions; a binary file under one of them is left to the built-in viewer.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    !head.contains(&0)
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark.
    start: u64,
    language: Language,
    lines: LineIndex,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once and offers `highlighted`.
    pub fn open(mut src: S) -> Result<(Self, Document), Failure> {
        let start = if src.size() >= 3 && src.read_at(0, 3)? == BOM {
            3
        } else {
            0
        };
        let name = src.name();
        let known = name
            .rsplit_once('.')
            .and_then(|(_, extension)| Language::from_extension(extension));
        let language = match known {
            Some(language) => language,
            None => {
                let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
                Language::of("", &src.read_at(start, length)?)
            }
        };
        let state = LineState::new(language.root());
        let mut lines = LineIndex::new(start, &state);
        let line_count = {
            let reader = Reader::new(&mut src, start, OPEN_CHUNK);
            let mut lexer = Lexer::resume(reader, 0, state, language.php());
            while lexer
                .next_with(&mut |line, offset, state| lines.line_start(line, offset, state))?
                .is_some()
            {}
            lexer.line_count()
        };
        lines.finish(line_count);
        let viewer = Self {
            src,
            start,
            language,
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

    /// The language the document is read in.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// The number of lines of the text.
    #[must_use]
    pub fn line_count(&self) -> u64 {
        self.lines.lines()
    }

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes()
    }

    /// `text`: writes the file from after its byte-order mark into `out` as UTF-8, a byte that
    /// is not UTF-8 as U+FFFD.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        utf8::write_lossy(&mut self.src, self.start, OPEN_CHUNK, out)?;
        Ok("utf-8".to_owned())
    }

    /// `tokens`: the tokens of lines `first_line..first_line + count`, one list per line that
    /// exists.
    pub fn tokens(&mut self, first_line: u32, count: u32) -> Result<Vec<Vec<Token>>, Failure> {
        let first = u64::from(first_line);
        let wanted = u64::from(count).min(self.lines.lines().saturating_sub(first));
        if wanted == 0 {
            return Ok(Vec::new());
        }
        let php = self.language.php();
        tokens::line_tokens(&mut self.src, &self.lines, php, first, wanted)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
