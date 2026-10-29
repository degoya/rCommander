//! rCommander's bundled configuration file viewer (RC-100-22): INI and its kin (`.inf`, `.cfg`,
//! `.conf`, `.editorconfig`, `.gitconfig`), Windows registry exports, `.env`, Java properties,
//! TOML, `.gitignore` and `.gitattributes`, coloured like an editor (`highlighted`) or as their
//! sections and keys (`tree`), for files of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the batch viewer's design, with a tree beside it:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's state
//!   and the entries before them ([`lines`]), and the sections and tables (`src/outline.rs`).
//!   Nothing scales with the file but the index, and the index is bounded
//!   ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file; [`Viewer::children`] reads a level's entries from the
//!   nearest checkpoint before them.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** The file is read in the encoding of the setting `encoding` (`auto`:
//!   a byte-order mark's, else UTF-8 when the first 64 KiB are, else Windows-1252), decoded by
//!   `rc-plugin-text`; `text` writes it as UTF-8 and the tokens' offsets are UTF-8 bytes of it.
//!
//! The format is chosen by the file's name ([`types::Format::from_name`]).
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

mod entry;
#[cfg(target_arch = "wasm32")]
mod guest;
mod lexer;
pub mod lines;
mod outline;
mod reader;
mod text;
pub mod tokens;
pub mod tree;
pub mod types;

#[cfg(test)]
mod test_support;

use entry::{LineKind, line_kind};
use lexer::{LineState, lex_line};
use lines::LineIndex;
use outline::{Outline, OutlineBuilder};
use rc_plugin_text::{Encoding, detect};
use reader::{Newline, Reader};
use text::decode_line;
use tree::{Doc, Tree};
use types::{Document, Failure, Format, Output, Sink, Source, Token, TreeNode};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index of one document takes. `manifest.toml`'s limits leave room for the
/// windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 16 << 20;
/// The first bytes `auto` decides the encoding by; the open pass goes on from them.
const HEAD_BYTES: u64 = 64 * 1024;
/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;

/// Whether the file looks like text: a UTF-16 byte-order mark, or no NUL byte in its first 4 KiB.
/// rCommander matches the manifest's extensions itself; of `.lock` files only `Cargo.lock` is
/// TOML, so every other is declined. This is `probe`.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let name = src.name().to_ascii_lowercase();
    if name.ends_with(".lock") && name != "cargo.lock" {
        return false;
    }
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    let utf16 = matches!(
        detect(&head, Encoding::Windows1252).0,
        Encoding::Utf16Le | Encoding::Utf16Be
    );
    utf16 || !head.contains(&0)
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    format: Format,
    /// The encoding the file is read in, `auto` resolved.
    encoding: Encoding,
    /// The offset of the text's first byte: after a byte-order mark of that encoding.
    start: u64,
    lines: LineIndex,
    outline: Outline,
    tree: Tree,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once, in the encoding `setting` names (`None`: `auto`), and offers
    /// `highlighted`, then `tree`.
    pub fn open(mut src: S, setting: Option<Encoding>) -> Result<(Self, Document), Failure> {
        let format = Format::from_name(&src.name());
        let head_length = u32::try_from(src.size().min(HEAD_BYTES)).unwrap_or(0);
        let head = if head_length > 0 {
            src.read_at(0, head_length)?
        } else {
            Vec::new()
        };
        let (found, bom) = detect(&head, Encoding::Windows1252);
        let encoding = setting.unwrap_or(found);
        // A byte-order mark of another encoding than the one chosen is text of it.
        let start = if encoding == found { bom as u64 } else { 0 };
        let mut lines = LineIndex::new(start);
        let mut outline = OutlineBuilder::new(format == Format::Toml);
        let (count, entries) = {
            let mut reader =
                Reader::new(&mut src, start, Newline::of(encoding), OPEN_CHUNK).with_head(head);
            let (mut raw, mut text, mut tokens) = (Vec::new(), String::new(), Vec::new());
            let mut state = LineState::default();
            let (mut count, mut entries) = (0u64, 0u64);
            while let Some(capped) = reader.read_line(tokens::MAX_LINE_BYTES, &mut raw)? {
                text.clear();
                decode_line(&raw, capped, encoding, &mut text);
                tokens.clear();
                let before = state;
                state = lex_line(text.as_bytes(), state, format, Some(&mut tokens));
                match line_kind(&text, before, &tokens) {
                    LineKind::Header { array, name } => outline.header(array, name, entries),
                    LineKind::Entry => entries += 1,
                    LineKind::Other => {}
                }
                count += 1;
                lines.line_start(count, reader.offset(), state, entries);
            }
            (count, entries)
        };
        lines.finish(count);
        let viewer = Self {
            src,
            format,
            encoding,
            start,
            lines,
            outline: outline.finish(entries),
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

    /// The encoding the file is read in: the setting's, `auto` resolved.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// The syntax the file is read in, by its name.
    #[must_use]
    pub fn format(&self) -> Format {
        self.format
    }

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes() + self.outline.memory_bytes() + self.tree.memory_bytes()
    }

    /// `text`: writes the file from after its byte-order mark into `out`, as UTF-8.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        text::write(&mut self.src, self.start, self.encoding, out)?;
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
        let doc = Doc {
            format: self.format,
            encoding: self.encoding,
            lines: &self.lines,
            outline: &self.outline,
        };
        tokens::line_tokens(&mut self.src, doc, first, wanted)
    }

    /// `children`: the top level for `None`, else the children of the node with that id.
    pub fn children(&mut self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        let doc = Doc {
            format: self.format,
            encoding: self.encoding,
            lines: &self.lines,
            outline: &self.outline,
        };
        self.tree.children(&mut self.src, doc, node)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
