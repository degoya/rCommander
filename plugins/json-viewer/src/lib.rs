//! rCommander's bundled JSON viewer (RC-100-11): JSON coloured like an editor (`highlighted`)
//! or unfolded as a tree (`tree`), for files of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)"):
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts ([`lines`]), and
//!   the ends, child counts and anchors of the large values ([`structure`]). Nothing scales
//!   with the file but the index, and the index is bounded ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] reads from the nearest
//!   checkpoint, never the whole file; [`Viewer::children`] reads one container's children, at
//!   most a page of them.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** A byte that is not is written as U+FFFD, and token offsets count the
//!   text written (`src/utf8.rs`), so they fall on its character boundaries.
//!
//! The setting `format` (`as_is` or `pretty`) chooses the text: the file's own lines, or the
//! JSON indented two spaces a level, since a minified file is otherwise one line.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32` (RC-100-12). The types in [`types`] mirror the viewer world one to one for that.

#[cfg(target_arch = "wasm32")]
mod guest;
pub mod lexer;
pub mod lines;
pub mod pretty;
mod reader;
pub mod structure;
mod text;
pub mod tokens;
pub mod tree;
pub mod types;
mod utf8;
mod walk;

#[cfg(test)]
mod test_support;

use lexer::Lexer;
use lines::LineIndex;
use pretty::LayoutState;
use reader::{Bytes, Reader};
use structure::{Builder, ParseError, Structure};
use tree::Tree;
use types::{Document, Failure, Format, Output, Sink, Source, Token, TreeNode};

/// The window `open` reads through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index of one document takes: line checkpoints, structure and range
/// nodes together. `manifest.toml`'s `memory_bytes` leaves room for the windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 16 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Whether the file looks like JSON: its first byte after a byte-order mark and whitespace
/// starts a JSON value. rCommander matches the manifest's extensions itself; this is `probe`.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(4096)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    let head = head.strip_prefix(BOM).unwrap_or(&head);
    head.iter()
        .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .is_some_and(|byte| {
            matches!(
                byte,
                b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
            )
        })
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    format: Format,
    /// The offset of the text's first byte: after a byte-order mark.
    start: u64,
    lines: LineIndex,
    structure: Structure,
    tree: Tree,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once and offers `highlighted`, then `tree`.
    pub fn open(mut src: S, format: Format) -> Result<(Self, Document), Failure> {
        let start = if src.size() >= 3 && src.read_at(0, 3)? == BOM {
            3
        } else {
            0
        };
        let mut lines = LineIndex::new(start);
        let (structure, line_count) = {
            let mut lexer = Lexer::new(Reader::new(&mut src, start, OPEN_CHUNK));
            let mut builder = Builder::new();
            let mut layout = LayoutState::default();
            let mut pretty_lines = 0u64;
            let mut saw_lexeme = false;
            let indented = format == Format::Pretty;
            loop {
                let lexeme = lexer.next_with(&mut |line, offset| {
                    if !indented {
                        lines.line_start(line, offset, LayoutState::default());
                    }
                })?;
                let Some(lexeme) = lexeme else { break };
                saw_lexeme = true;
                builder.feed(&lexeme);
                if indented {
                    let before = layout;
                    let mut first = true;
                    for piece in layout.feed(&lexeme).iter() {
                        if let pretty::Piece::Break(_) = piece {
                            pretty_lines += 1;
                            if first {
                                lines.line_start(pretty_lines, lexeme.start, before);
                                first = false;
                            }
                        }
                    }
                }
            }
            let eof = lexer.bytes_mut().offset();
            let (line, column) = lexer.position();
            let line_count = if indented {
                if saw_lexeme { pretty_lines + 1 } else { 0 }
            } else {
                line_total(line, column, eof > start)
            };
            (builder.finish(eof, line, column), line_count)
        };
        lines.finish(line_count);
        let viewer = Self {
            src,
            format,
            start,
            lines,
            structure,
            tree: Tree::default(),
        };
        let document = viewer.document();
        Ok((viewer, document))
    }

    /// The outputs this document offers, `highlighted` first.
    #[must_use]
    pub fn document(&self) -> Document {
        Document {
            outputs: vec![Output::Highlighted, Output::Tree],
            title: None,
            pages: None,
        }
    }

    /// The number of lines of the text.
    #[must_use]
    pub fn line_count(&self) -> u64 {
        self.lines.lines()
    }

    /// Where the JSON first stops being valid.
    #[must_use]
    pub fn parse_error(&self) -> Option<&ParseError> {
        self.structure.error.as_ref()
    }

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes() + self.structure.memory_bytes() + self.tree.memory_bytes()
    }

    /// `text`: writes the text into `out` and names its encoding.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        match self.format {
            Format::AsIs => text::copy(&mut self.src, self.start, out)?,
            Format::Pretty => text::pretty(&mut self.src, self.start, out)?,
        }
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
        let fetched = match self.format {
            Format::AsIs => text::as_is_lines(&mut self.src, &self.lines, first, wanted)?,
            Format::Pretty => text::pretty_lines(&mut self.src, &self.lines, first, wanted)?,
        };
        let mut all = Vec::new();
        for (index, line) in fetched.iter().enumerate().take(wanted as usize) {
            let colon = !line.capped
                && tokens::colon_follows(
                    fetched
                        .iter()
                        .skip(index + 1)
                        .map(|following| following.bytes.as_slice()),
                );
            let mut tokens = tokens::tokenize_line(&line.bytes, line.capped, colon);
            utf8::to_text(&line.bytes, &mut tokens);
            all.push(tokens);
        }
        Ok(all)
    }

    /// `children`: the top level for `None`, else the children of the node with that id.
    pub fn children(&mut self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        self.tree.children(&mut self.src, &self.structure, node)
    }
}

/// The lines of a file whose lexer ended at `line` and `column`: a last line without a line
/// break counts, an empty one after the last break does not.
fn line_total(line: u64, column: u64, any: bool) -> u64 {
    match (any, column) {
        (false, _) => 0,
        (true, 1) => line - 1,
        (true, _) => line,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
