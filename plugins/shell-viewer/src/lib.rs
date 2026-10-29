//! rCommander's bundled shell script viewer (RC-100-18): sh, bash, zsh and ksh coloured like an
//! editor (`highlighted`), for files of any size the viewer accepts.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)") - the YAML viewer's design, with the shell's nesting in the lexer's state:
//!
//! - **The file is a handle, read in windows.** [`Viewer::open`] streams it once, in windows of
//!   at most 1 MiB, and keeps only an index: checkpoints of the line starts with the lexer's
//!   state there ([`lines`]) - the open quotes, substitutions and expansions, a heredoc and its
//!   marker, a command continued with `\`. Nothing scales with the file but the index, and the
//!   index is bounded ([`INDEX_BUDGET_BYTES`]).
//! - **Only the lines on screen are asked for.** [`Viewer::tokens`] lexes from the nearest
//!   checkpoint, never the whole file.
//! - **Classes, not colours.** A token names a class; the theme colours it.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//! - **The text is UTF-8.** A byte that is not is written as U+FFFD, and token offsets count
//!   the text written, so they always fall on its character boundaries.
//!
//! The text is the file's own, so the plugin has no setting.
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
mod utf8;

#[cfg(test)]
mod test_support;

use lexer::{Lexer, QuoteOpen};
use lines::LineIndex;
use reader::Reader;
use types::{Document, Failure, Output, Sink, Source, Token};

/// The window `open` and `text` read through: the most one `read-at` serves.
pub const OPEN_CHUNK: u32 = 1 << 20;
/// The most memory the index of one document takes. `manifest.toml`'s limits leave room for the
/// windows beside it.
pub const INDEX_BUDGET_BYTES: usize = 8 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The bytes `probe` looks at.
const PROBE_BYTES: u64 = 4096;
/// The interpreters a shebang may name for the file to be a shell script.
const SHELLS: [&[u8]; 9] = [
    b"sh", b"bash", b"zsh", b"ksh", b"ksh93", b"mksh", b"pdksh", b"dash", b"ash",
];

/// Whether the file is a shell script: text (no NUL byte in its first 4 KiB) whose shebang, if
/// it has one, names a shell. rCommander offers the file by the manifest's extensions and
/// shebang magics; a `#!/usr/bin/env python3` named `.sh`, or a magic's prefix of another
/// interpreter, is left to the built-in viewer.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(4096);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    if head.contains(&0) {
        return false;
    }
    let text = head.strip_prefix(BOM).unwrap_or(&head);
    text.strip_prefix(b"#!").is_none_or(shebang_names_a_shell)
}

/// Whether the shebang line after `#!` runs a shell, directly or through `env`.
fn shebang_names_a_shell(rest: &[u8]) -> bool {
    let line = rest.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let mut words = line
        .split(u8::is_ascii_whitespace)
        .filter(|word| !word.is_empty());
    let Some(mut program) = words.next().map(base_name) else {
        return false;
    };
    if program == b"env" {
        // `env -S bash`, `env VAR=1 sh`: the first word that is neither an option nor a variable.
        let Some(after) = words.find(|word| !word.starts_with(b"-") && !word.contains(&b'='))
        else {
            return false;
        };
        program = base_name(after);
    }
    SHELLS.contains(&program)
}

/// What follows the last `/` of a path.
fn base_name(path: &[u8]) -> &[u8] {
    path.rsplit(|byte| *byte == b'/').next().unwrap_or(path)
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    /// The offset of the text's first byte: after a byte-order mark.
    start: u64,
    lines: LineIndex,
    /// The quotes the file leaves open at its end: their opening shows as `invalid`.
    unclosed: Vec<QuoteOpen>,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file once and offers `highlighted`.
    pub fn open(mut src: S) -> Result<(Self, Document), Failure> {
        let start = if src.size() >= 3 && src.read_at(0, 3)? == BOM {
            3
        } else {
            0
        };
        let mut lines = LineIndex::new(start);
        let (line_count, unclosed) = {
            let mut lexer = Lexer::new(Reader::new(&mut src, start, OPEN_CHUNK));
            while lexer
                .next_with(&mut |line, offset, state| lines.line_start(line, offset, state))?
                .is_some()
            {}
            (lexer.line_count(), lexer.open_quotes().to_vec())
        };
        lines.finish(line_count);
        let viewer = Self {
            src,
            start,
            lines,
            unclosed,
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

    /// The memory the index takes now.
    #[must_use]
    pub fn index_bytes(&self) -> usize {
        self.lines.memory_bytes() + self.unclosed.capacity() * size_of::<QuoteOpen>()
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
        tokens::line_tokens(&mut self.src, &self.lines, &self.unclosed, first, wanted)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
