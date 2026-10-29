//! rCommander's bundled diff and patch viewer (RC-100-24): unified, combined and context diffs
//! coloured (`highlighted`), and the patch's files with their hunks as a tree (`tree`), for files
//! of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the shell viewer's design, with a diff's lines classified one by one:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index and the tree: checkpoints of the line starts with the
//!   state there ([`lines`]) - before the first diff, between files, or how many lines a hunk
//!   still promises - and per file and hunk where its name is in the file. Both are bounded
//!   ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] classifies from the nearest
//!   checkpoint, never the whole file; [`Viewer::children`] reads the names it shows.
//! - **Classes, not colours.** A token names a class; the theme colours it: an added line
//!   `number` (green), a removed one `invalid` (red).
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** A byte that is not is written as U+FFFD, and token offsets count
//!   the text written, so they always fall on its character boundaries.
//!
//! The text is the file's own, so the plugin has no setting.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

mod classify;
#[cfg(target_arch = "wasm32")]
mod guest;
pub mod lines;
mod reader;
pub mod tokens;
pub mod tree;
pub mod types;
mod utf8;

#[cfg(test)]
mod test_support;

use classify::{LineState, classify};
use lines::LineIndex;
use reader::{End, LineReader};
use tokens::KEEP_BYTES;
use tree::Tree;
use types::{Document, Failure, Output, Sink, Source, Token, TreeNode};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index and the tree of one document take together. `manifest.toml`'s
/// limits leave room for the windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 8 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;
/// The extensions of `manifest.toml`: a file offered by one of them is taken as it is.
const EXTENSIONS: [&str; 3] = ["diff", "patch", "rej"];
/// The hex digits of a commit id after `From ` in `git format-patch`'s first line.
const COMMIT_DIGITS: usize = 40;

/// Whether the file is a diff: text (no NUL byte in its first 4 KiB) that came by one of the
/// manifest's extensions, or - claimed by a magic - starts with `diff `, with `--- ` and a
/// `+++ ` line after it, or with `From <commit> ` as `git format-patch` writes it. A mail, a
/// YAML document or a text that only shares the prefix is declined, for the next plugin or the
/// built-in viewer.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    if head.contains(&0) {
        return false;
    }
    let extension = src
        .name()
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    if extension.is_some_and(|extension| EXTENSIONS.contains(&extension.as_str())) {
        return true;
    }
    let text = head.strip_prefix(BOM).unwrap_or(&head);
    text.starts_with(b"diff ") || starts_unified(text) || starts_format_patch(text)
}

/// `--- <old>` and on the next line `+++ <new>`: a plain `diff -u` without its command line.
fn starts_unified(text: &[u8]) -> bool {
    text.starts_with(b"--- ")
        && text
            .iter()
            .position(|byte| *byte == b'\n')
            .and_then(|end| text.get(end + 1..))
            .is_some_and(|next| next.starts_with(b"+++ "))
}

/// `From <40 hex digits> `.
fn starts_format_patch(text: &[u8]) -> bool {
    text.strip_prefix(b"From ")
        .and_then(|rest| rest.get(..=COMMIT_DIGITS))
        .is_some_and(|id| {
            id.iter().take(COMMIT_DIGITS).all(u8::is_ascii_hexdigit)
                && id.get(COMMIT_DIGITS) == Some(&b' ')
        })
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark.
    start: u64,
    lines: LineIndex,
    tree: Tree,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once and offers `highlighted` and `tree`.
    pub fn open(mut src: S) -> Result<(Self, Document), Failure> {
        let start = if src.size() >= 3 && src.read_at(0, 3)? == BOM {
            3
        } else {
            0
        };
        let mut lines = LineIndex::new(start);
        let mut tree = Tree::default();
        let line_count = {
            let mut reader = LineReader::new(&mut src, start, OPEN_CHUNK);
            let mut head = Vec::with_capacity(KEEP_BYTES);
            let mut state = LineState::default();
            let mut line = 0u64;
            loop {
                let offset = reader.offset();
                let Some(end) = reader.next_line(&mut head, KEEP_BYTES)? else {
                    break;
                };
                if end == End::Cut {
                    reader.skip_line()?;
                }
                lines.line_start(line, offset, state);
                let text = tokens::content(&head, end);
                let kind = classify(&mut state, text);
                tree.line(kind, text, offset);
                line += 1;
            }
            line
        };
        lines.finish(line_count);
        let viewer = Self {
            src,
            start,
            lines,
            tree,
        };
        let document = viewer.document();
        Ok((viewer, document))
    }

    /// The outputs this document offers: `highlighted` first, then `tree`.
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

    /// The memory the index and the tree take now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes() + self.tree.memory_bytes()
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
        tokens::line_tokens(&mut self.src, &self.lines, first, wanted)
    }

    /// `children`: the files for `None`, else the hunks of a file or the nodes of a range.
    pub fn children(&mut self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        self.tree.children(&mut self.src, node)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
