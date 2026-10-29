//! rCommander's bundled YAML viewer (RC-100-16): YAML coloured like an editor (`highlighted`)
//! or unfolded as a tree (`tree`), for files of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the JSON viewer's design, with YAML's context in the lexer's state:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's
//!   state there ([`lines`]), and the ends, child counts and marks of the large collections and
//!   values ([`structure`]). Nothing scales with the file but the index, and the index is
//!   bounded ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file; [`Viewer::children`] reads one collection's children, at
//!   most a page of them.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** A byte that is not is written as U+FFFD, and token offsets count the
//!   text written (`src/utf8.rs`), so they fall on its character boundaries.
//! - **Nothing is resolved.** Anchors, aliases, tags and merge keys are shown as written.
//!
//! The text is the file's own: YAML is not reformatted, so the plugin has no setting.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

mod events;
#[cfg(target_arch = "wasm32")]
mod guest;
mod lexer;
pub mod lines;
mod reader;
pub mod structure;
pub mod tokens;
pub mod tree;
pub mod types;
mod utf8;
mod walk;

#[cfg(test)]
mod test_support;

use events::Events;
use lexer::Lexer;
use lines::LineIndex;
use reader::{Bytes, Reader};
use structure::{Builder, ParseError, Structure};
use tree::Tree;
use types::{Document, Failure, Output, Sink, Source, Token, TreeNode};
use utf8::TextWriter;

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index of one document takes: line checkpoints, structure and range
/// nodes together. `manifest.toml`'s limits leave room for the windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 16 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;

/// Whether the file looks like text: no NUL byte in its first 4 KiB. YAML has no signature, and
/// rCommander matches the manifest's extensions itself; this is `probe`.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    src.read_at(0, length).is_ok_and(|head| !head.contains(&0))
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark.
    start: u64,
    lines: LineIndex,
    structure: Structure,
    tree: Tree,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once and offers `highlighted`, then `tree`.
    pub fn open(mut src: S) -> Result<(Self, Document), Failure> {
        let start = if src.size() >= 3 && src.read_at(0, 3)? == BOM {
            3
        } else {
            0
        };
        let mut lines = LineIndex::new(start);
        let (structure, line_count) = {
            let lexer = Lexer::new(Reader::new(&mut src, start, OPEN_CHUNK));
            let mut events = Events::new(lexer, u64::MAX, false);
            let mut builder = Builder::new(start);
            while let Some(event) = events.next_with(&mut |line, offset, state| {
                lines.line_start(line, offset, state);
            })? {
                builder.feed(&event);
            }
            let lexer = events.lexer_mut();
            let eof = lexer.bytes_mut().offset();
            let (line, column) = lexer.position();
            let state = lexer.state();
            (
                builder.finish(eof, line, column, state),
                line_total(line, column, eof > start),
            )
        };
        lines.finish(line_count);
        let viewer = Self {
            src,
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

    /// Where the YAML first stops being well formed.
    #[must_use]
    pub fn parse_error(&self) -> Option<&ParseError> {
        self.structure.error.as_ref()
    }

    /// The number of documents in the stream.
    #[must_use]
    pub fn document_count(&self) -> u64 {
        self.structure.documents.count
    }

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes() + self.structure.memory_bytes() + self.tree.memory_bytes()
    }

    /// `text`: writes the file from after its byte-order mark into `out`, as it is.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let mut reader = Reader::new(&mut self.src, self.start, OPEN_CHUNK);
        let size = reader.source().size();
        let mut writer = TextWriter::new(out);
        reader.copy_range(self.start, size, &mut |bytes| writer.push(bytes))?;
        writer.finish()?;
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
        tokens::line_tokens(&mut self.src, &self.lines, first, wanted)
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
