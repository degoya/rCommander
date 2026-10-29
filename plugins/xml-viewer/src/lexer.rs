//! The XML lexer, one for every pass: the structure and line index of `open`, the pretty
//! printer, the tree's walks and the tokens of one line.
//!
//! A lexeme never spans a line. A construct that does - a comment, a CDATA section, a processing
//! instruction, a DOCTYPE, an attribute value, the attributes of a tag - is lexed a piece per
//! line, and the lexer's [`Mode`] between two lexemes says which construct goes on. That mode is
//! the whole state at a line's start, which is what a checkpoint keeps and why `tokens` can start
//! at any checkpoint. No DOM, no DTD, no entity expansion: the lexer only tells markup apart.

mod scan;

use crate::reader::Bytes;
use crate::types::Failure;

/// What a lexeme is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `<` before an element's name.
    Open,
    /// `</` before an end tag's name.
    EndOpen,
    /// An element's name, in a start or an end tag.
    Name,
    AttrName,
    /// `=` between an attribute's name and its value.
    Eq,
    /// A piece of an attribute value, its quotes included; `closed` once the closing quote is.
    Value {
        closed: bool,
    },
    /// `>` closing a start tag.
    Close,
    /// `>` closing an end tag.
    EndClose,
    /// `/>` closing an empty element.
    EmptyClose,
    /// Character data between markup, from its first byte that is not whitespace.
    Text,
    /// `&name;`, `&#n;` or `&#xh;`, in text or in an attribute value.
    Entity,
    /// A piece of `<!-- ... -->`.
    Comment {
        closed: bool,
    },
    /// A piece of `<? ... ?>`.
    Pi {
        closed: bool,
    },
    /// A piece of `<!DOCTYPE ...>` or another declaration.
    Doctype {
        closed: bool,
    },
    /// `<![CDATA[`.
    CDataOpen,
    /// A piece of a CDATA section's content; `closed` when it ends with the `]]>`.
    CData {
        closed: bool,
    },
    Invalid(Problem),
}

/// Why a lexeme is `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    BadMarkup,
    BadEntity,
    Unexpected,
}

impl Problem {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::BadMarkup => "invalid markup",
            Self::BadEntity => "invalid entity reference",
            Self::Unexpected => "unexpected character in a tag",
        }
    }
}

/// What the lexer is inside of between two lexemes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Between markup: text, entities and the next `<`.
    #[default]
    Content,
    /// Right after `<` or `</` (`end`): the name follows.
    Name {
        end: bool,
    },
    /// In a start tag after its name: attributes, `>` or `/>`.
    Tag,
    /// In an end tag after its name: `>`.
    EndTag,
    /// In an attribute value opened by this quote.
    Value(u8),
    Comment,
    Pi,
    /// In a declaration, `depth` brackets of an internal subset deep; `quote` is the quote of a
    /// literal it is inside, 0 for none.
    Doctype {
        depth: u8,
        quote: u8,
    },
    CData,
}

impl Mode {
    /// A construct whose whitespace is its own: only line breaks lie between its pieces.
    pub(crate) fn is_construct(self) -> bool {
        matches!(
            self,
            Self::Value(_) | Self::Comment | Self::Pi | Self::Doctype { .. } | Self::CData
        )
    }
}

/// Where a lexer starts again: its mode, and the whitespace it passed before the next lexeme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LexState {
    pub(crate) mode: Mode,
    /// A line break came before the next lexeme.
    pub(crate) newline: bool,
    /// Whitespace came before the next lexeme.
    pub(crate) spaced: bool,
}

impl LexState {
    /// The state at the start of a line in `mode`.
    pub(crate) fn at(mode: Mode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }
}

/// One lexeme: bytes `start..end` of its stream, where it starts (1-based line and column in
/// characters, counted only by a lexer that started at the beginning), and the state it was
/// lexed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme {
    pub(crate) kind: Kind,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) line: u64,
    pub(crate) column: u64,
    pub(crate) state: LexState,
}

pub(crate) struct Lexer<B: Bytes> {
    bytes: B,
    mode: Mode,
    newline: bool,
    spaced: bool,
    line: u64,
    column: u64,
    capture_cap: usize,
    captured: Vec<u8>,
}

/// A byte that may start an XML name: a letter, `_`, `:` or any byte of a non-ASCII character.
pub(crate) fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':') || byte >= 0x80
}

impl<B: Bytes> Lexer<B> {
    pub(crate) fn new(bytes: B) -> Self {
        Self::resume(bytes, LexState::default())
    }

    /// A lexer whose next lexeme starts in `state`.
    pub(crate) fn resume(bytes: B, state: LexState) -> Self {
        Self {
            bytes,
            mode: state.mode,
            newline: state.newline,
            spaced: state.spaced,
            line: 1,
            column: 1,
            capture_cap: 0,
            captured: Vec::new(),
        }
    }

    /// Keeps the first `cap` bytes of every lexeme in [`Self::captured`].
    pub(crate) fn with_capture(mut self, cap: usize) -> Self {
        self.capture_cap = cap;
        self
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut B {
        &mut self.bytes
    }

    /// Starts again in `state`, after the caller moved the bytes.
    pub(crate) fn set_state(&mut self, state: LexState) {
        self.mode = state.mode;
        self.newline = state.newline;
        self.spaced = state.spaced;
    }

    /// The first bytes of the last lexeme, at most the capture's cap of them.
    pub(crate) fn captured(&self) -> &[u8] {
        &self.captured
    }

    /// The 1-based line and column of the next byte.
    pub(crate) fn position(&self) -> (u64, u64) {
        (self.line, self.column)
    }

    /// The mode the next lexeme starts in.
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    fn take(&mut self, byte: u8) {
        self.bytes.bump();
        if byte & 0xC0 != 0x80 {
            self.column += 1;
        }
        if self.captured.len() < self.capture_cap {
            self.captured.push(byte);
        }
    }

    /// Skips the whitespace before the next lexeme - in a construct only line breaks - telling
    /// `on_line` the number (0-based), offset and mode of every line it starts, and returns the
    /// next byte.
    pub(crate) fn skip(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64, Mode),
    ) -> Result<Option<u8>, Failure> {
        let construct = self.mode.is_construct();
        while let Some(byte) = self.bytes.peek()? {
            let skipped = match byte {
                b'\r' | b'\n' => true,
                b' ' | b'\t' => !construct,
                _ => false,
            };
            if !skipped {
                return Ok(Some(byte));
            }
            self.bytes.bump();
            self.spaced = true;
            if byte == b'\n' {
                self.line += 1;
                self.column = 1;
                self.newline = true;
                on_line(self.line - 1, self.bytes.offset(), self.mode);
            } else {
                self.column += 1;
            }
        }
        Ok(None)
    }

    pub(crate) fn next(&mut self) -> Result<Option<Lexeme>, Failure> {
        self.next_with(&mut |_, _, _| {})
    }

    /// The next lexeme; `on_line` hears of the lines started on the way.
    pub(crate) fn next_with(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64, Mode),
    ) -> Result<Option<Lexeme>, Failure> {
        let Some(first) = self.skip(on_line)? else {
            return Ok(None);
        };
        if first == b'<' && matches!(self.mode, Mode::Tag | Mode::EndTag | Mode::Value(_)) {
            // Markup ends the tag or the value it interrupts; the structure names the error.
            self.mode = Mode::Content;
        }
        self.captured.clear();
        let state = LexState {
            mode: self.mode,
            newline: self.newline,
            spaced: self.spaced,
        };
        self.newline = false;
        self.spaced = false;
        let (start, line, column) = (self.bytes.offset(), self.line, self.column);
        let kind = match self.mode {
            Mode::Content => self.content(first)?,
            Mode::Name { end } => self.name(end, first)?,
            Mode::Tag => self.in_tag(first)?,
            Mode::EndTag => self.in_end_tag(first)?,
            Mode::Value(_) if first == b'&' => self.entity()?,
            Mode::Value(quote) => self.value(quote)?,
            Mode::Comment => self.comment()?,
            Mode::Pi => self.pi()?,
            Mode::Doctype { .. } => self.doctype()?,
            Mode::CData => self.cdata()?,
        };
        Ok(Some(Lexeme {
            kind,
            start,
            end: self.bytes.offset(),
            line,
            column,
            state,
        }))
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
