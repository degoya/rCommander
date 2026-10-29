//! rCommander's bundled CSV viewer (RC-100-17): CSV and TSV with every column in its own colour
//! (`highlighted`), or the records unfolded as a tree (`tree`), for files of any size the viewer
//! accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the JSON viewer's design, with the CSV state a line starts in kept as the YAML
//! viewer keeps its lexer's:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the state there
//!   (inside a quoted field, which column) and of the record starts ([`lines`]). Nothing scales
//!   with the file but the index, and the index is bounded ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file; [`Viewer::children`] reads a page of records from the
//!   nearest record checkpoint.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//!
//! The settings `separator`, `header` and `encoding` ([`types::Settings`]) are read once, when a
//! document opens; each `auto` is decided from the file ([`detect`], and the open pass for the
//! encoding). Whatever the file is written in, the text the host gets is UTF-8 and the tokens
//! count its bytes.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

pub mod detect;
pub mod encoding;
#[cfg(target_arch = "wasm32")]
mod guest;
pub mod lines;
mod machine;
mod reader;
mod scan;
mod text;
pub mod tokens;
pub mod tree;
pub mod types;
mod walk;

#[cfg(test)]
mod test_support;

use encoding::Encoding;
use lines::{LineIndex, RecordIndex};
use tokens::Layout;
use tree::{Table, Tree};
use types::{
    Document, EncodingSetting, Failure, HeaderSetting, Output, SeparatorSetting, Settings, Sink,
    Source, Token, TreeNode,
};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index of one document takes: line and record checkpoints, the column
/// names and range nodes together. `manifest.toml`'s limits leave room for the windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 16 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;

/// Whether the file looks like text: no NUL byte in its first 4 KiB. CSV has no signature, and
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
    sep: u8,
    encoding: Encoding,
    /// The header's fields; empty without a header.
    names: Vec<Vec<u8>>,
    header: bool,
    lines: LineIndex,
    records: RecordIndex,
    unclosed_line: Option<u64>,
    tree: Tree,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once and offers `highlighted`, then `tree`.
    pub fn open(mut src: S, settings: Settings) -> Result<(Self, Document), Failure> {
        let size = src.size();
        let start = if size >= 3 && src.read_at(0, 3)? == BOM {
            3
        } else {
            0
        };
        let head_length = u32::try_from((size - start).min(u64::from(detect::HEAD_BYTES)))
            .unwrap_or(detect::HEAD_BYTES);
        let mut head = if head_length > 0 {
            src.read_at(start, head_length)?
        } else {
            Vec::new()
        };
        head.truncate(head_length as usize);
        let sep = match settings.separator {
            SeparatorSetting::Fixed(sep) => sep,
            SeparatorSetting::Auto => detect::separator(&src.name(), &head),
        };
        let mut names = detect::first_record(&head, sep);
        let header = match settings.header {
            HeaderSetting::Yes => !names.is_empty(),
            HeaderSetting::No => false,
            HeaderSetting::Auto => detect::looks_like_header(&names),
        };
        if !header {
            names = Vec::new();
        }
        // A byte-order mark says UTF-8; only `auto` without one needs the whole file looked at.
        let check_utf8 = settings.encoding == EncodingSetting::Auto && start == 0;
        let scan = scan::scan(&mut src, start, sep, check_utf8)?;
        let encoding = match settings.encoding {
            EncodingSetting::Utf8 => Encoding::Utf8,
            EncodingSetting::Windows1252 => Encoding::Windows1252,
            EncodingSetting::Auto if scan.utf8 => Encoding::Utf8,
            EncodingSetting::Auto => Encoding::Windows1252,
        };
        let viewer = Self {
            src,
            start,
            sep,
            encoding,
            names,
            header,
            lines: scan.lines,
            records: scan.records,
            unclosed_line: scan.unclosed_line,
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
        self.lines.count()
    }

    /// The number of rows: the records after the header.
    #[must_use]
    pub fn row_count(&self) -> u64 {
        self.table().rows()
    }

    /// The separator in use.
    #[must_use]
    pub fn separator(&self) -> u8 {
        self.sep
    }

    /// The encoding the file is read in.
    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Whether the first record is the header.
    #[must_use]
    pub fn has_header(&self) -> bool {
        self.header
    }

    /// The line of a quote never closed, when there is one.
    #[must_use]
    pub fn unclosed_line(&self) -> Option<u64> {
        self.unclosed_line
    }

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes()
            + self.records.memory_bytes()
            + self.names.iter().map(Vec::capacity).sum::<usize>()
            + self.tree.memory_bytes()
    }

    fn table(&self) -> Table<'_> {
        Table {
            sep: self.sep,
            encoding: self.encoding,
            records: &self.records,
            skip: u64::from(self.header),
            names: &self.names,
        }
    }

    /// `text`: writes the text into `out`, as UTF-8, and names its encoding.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        text::write(&mut self.src, self.start, self.encoding, out)?;
        Ok("utf-8".to_owned())
    }

    /// `tokens`: the tokens of lines `first_line..first_line + count`, one list per line that
    /// exists.
    pub fn tokens(&mut self, first_line: u32, count: u32) -> Result<Vec<Vec<Token>>, Failure> {
        let first = u64::from(first_line);
        let wanted = u64::from(count).min(self.lines.count().saturating_sub(first));
        if wanted == 0 {
            return Ok(Vec::new());
        }
        let layout = Layout {
            sep: self.sep,
            encoding: self.encoding,
            unclosed_line: self.unclosed_line,
        };
        tokens::line_tokens(&mut self.src, &self.lines, layout, first, wanted)
    }

    /// `children`: the rows for `None`, else the children of the node with that id.
    pub fn children(&mut self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        let table = Table {
            sep: self.sep,
            encoding: self.encoding,
            records: &self.records,
            skip: u64::from(self.header),
            names: &self.names,
        };
        self.tree.children(&mut self.src, &table, node)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
