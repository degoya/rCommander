//! The YAML lexer, one for every pass: the structure and line index of `open`, the tree's walks
//! and the tokens of the lines on screen.
//!
//! A lexeme never spans a line. What YAML carries from one line into the next is the
//! [`LineState`]: the depth of the open flow collections, a quoted scalar still open, a block
//! scalar (`|`, `>`) the lines belong to while they are indented enough. The line index keeps
//! that state at every checkpoint, so `tokens` starts at any checkpoint without reading what
//! lies before it. Anchors, aliases and tags are lexemes of their own; nothing is resolved.

use crate::reader::Bytes;
use crate::types::Failure;

mod scalars;

/// What a lexeme is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A plain scalar; its kind (number, literal, text) is the reader's to tell.
    Plain,
    /// A quoted scalar, or its first line when it goes on.
    Quoted,
    /// A further line of a quoted scalar.
    QuotedLine,
    /// `|` or `>` with its indicators.
    BlockHeader,
    /// A line of a block scalar.
    BlockText,
    Comment,
    /// `---` and `...`
    DocStart,
    DocEnd,
    /// `%YAML`, `%TAG`
    Directive,
    /// `&name`, `*name`, `!tag`
    Anchor,
    Alias,
    Tag,
    /// `-` before a sequence item, `?` before an explicit key, `:` after a key
    Dash,
    Question,
    Colon,
    FlowOpen,
    FlowClose,
    Comma,
    Invalid(Problem),
}

/// Why a lexeme is `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    TabIndent,
    BadEscape,
    Reserved,
    StrayClose,
    BlockInFlow,
}

impl Problem {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::TabIndent => "tab in the indentation",
            Self::BadEscape => "invalid escape in a double-quoted scalar",
            Self::Reserved => "'@' and '`' cannot start a scalar",
            Self::StrayClose => "closing bracket without an opening one",
            Self::BlockInFlow => "a block scalar inside a flow collection",
        }
    }
}

/// The block scalar the next lines belong to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Block {
    /// The least indentation of a content line.
    pub(crate) min: u32,
    /// The indentation of the content, once its first line or an indicator named it.
    pub(crate) indent: Option<u32>,
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    /// Open flow collections (`[`, `{`).
    pub(crate) flow: u32,
    /// The quote of a quoted scalar still open, else 0.
    pub(crate) quote: u8,
    pub(crate) block: Option<Block>,
}

/// One lexeme: bytes `start..end` of its stream, and where it starts: the 1-based line and
/// column in characters (counted only by a lexer that started at the beginning), and `col`, its
/// byte column within the line, which is what indentation is measured in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme {
    pub(crate) kind: Kind,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) line: u64,
    pub(crate) column: u64,
    pub(crate) col: u32,
}

/// A position to come back to after looking ahead.
#[derive(Clone, Copy)]
struct Mark {
    offset: u64,
    col: u32,
    column: u64,
    captured: usize,
}

pub(crate) struct Lexer<B: Bytes> {
    bytes: B,
    line: u64,
    column: u64,
    col: u32,
    at_line_start: bool,
    state: LineState,
    /// A block header on this line: the state of the lines after it.
    next_block: Option<Block>,
    /// The current line's indentation.
    indent: u32,
    /// The column of the key or `-` a block scalar on this line belongs to.
    node_col: Option<u32>,
    /// The column of the last scalar on this line, the key when a `:` follows.
    scalar_col: Option<u32>,
    /// A quoted scalar just closed in a flow collection: a `:` right after it is an indicator.
    adjacent: bool,
    capture_cap: usize,
    captured: Vec<u8>,
}

/// Whether `next` ends an indicator (`-`, `?`, `:`, a document marker): a blank, the line's end,
/// or in a flow collection a flow indicator.
fn ends_indicator(next: Option<u8>, flow: bool) -> bool {
    match next {
        None | Some(b' ' | b'\t' | b'\r' | b'\n') => true,
        Some(b',' | b'[' | b']' | b'{' | b'}') => flow,
        Some(_) => false,
    }
}

impl<B: Bytes> Lexer<B> {
    /// A lexer at the start of the text.
    pub(crate) fn new(bytes: B) -> Self {
        Self::resume(bytes, 0, LineState::default())
    }

    /// A lexer at the start of 0-based line `line`, which starts in `state`.
    pub(crate) fn resume(bytes: B, line: u64, state: LineState) -> Self {
        Self {
            bytes,
            line: line + 1,
            column: 1,
            col: 0,
            at_line_start: true,
            state,
            next_block: None,
            indent: 0,
            node_col: None,
            scalar_col: None,
            adjacent: false,
            capture_cap: 0,
            captured: Vec::new(),
        }
    }

    /// A lexer at a node that starts at byte column `col` of a line in plain block context.
    pub(crate) fn mid_line(bytes: B, col: u32) -> Self {
        let mut lexer = Self::resume(bytes, 0, LineState::default());
        lexer.at_line_start = false;
        lexer.col = col;
        lexer.indent = col;
        lexer
    }

    /// Keeps the first `cap` bytes of every lexeme in [`Self::captured`].
    pub(crate) fn with_capture(mut self, cap: usize) -> Self {
        self.capture_cap = cap;
        self
    }

    /// Goes on at `offset`, the start of 0-based line `line` in `state`.
    pub(crate) fn seek_line(&mut self, offset: u64, line: u64, state: LineState) {
        self.bytes.seek(offset);
        self.line = line + 1;
        self.column = 1;
        self.col = 0;
        self.at_line_start = true;
        self.state = state;
        self.next_block = None;
        self.adjacent = false;
    }

    /// Goes on at `offset`, the start of a line in plain block context. Line numbers mean
    /// nothing after a jump; a new one tells the reader a line started.
    pub(crate) fn jump(&mut self, offset: u64) {
        self.seek_line(offset, self.line, LineState::default());
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut B {
        &mut self.bytes
    }

    /// The first bytes of the last lexeme, at most the capture's cap of them; a plain scalar's
    /// may run past its end into the blanks after it.
    pub(crate) fn captured(&self) -> &[u8] {
        &self.captured
    }

    /// The 1-based line and column of the next byte.
    pub(crate) fn position(&self) -> (u64, u64) {
        (self.line, self.column)
    }

    /// The state the lexer is in now.
    pub(crate) fn state(&self) -> LineState {
        self.state
    }

    fn take(&mut self, byte: u8) {
        self.bytes.bump();
        self.col = self.col.saturating_add(1);
        if byte & 0xC0 != 0x80 {
            self.column += 1;
        }
        if self.captured.len() < self.capture_cap {
            self.captured.push(byte);
        }
    }

    /// Takes a blank, which no lexeme keeps.
    fn skip(&mut self) {
        self.bytes.bump();
        self.col = self.col.saturating_add(1);
        self.column += 1;
    }

    fn mark(&self) -> Mark {
        Mark {
            offset: self.bytes.offset(),
            col: self.col,
            column: self.column,
            captured: self.captured.len(),
        }
    }

    fn restore(&mut self, mark: Mark) {
        self.bytes.seek(mark.offset);
        self.col = mark.col;
        self.column = mark.column;
        self.captured.truncate(mark.captured);
    }

    fn lexeme(&self, kind: Kind, start: Mark, line: u64, end: u64) -> Lexeme {
        Lexeme {
            kind,
            start: start.offset,
            end,
            line,
            column: start.column,
            col: start.col,
        }
    }

    pub(crate) fn next(&mut self) -> Result<Option<Lexeme>, Failure> {
        self.next_with(&mut |_, _, _| {})
    }

    /// The next lexeme; `on_line` hears of every line started on the way: its 0-based number,
    /// its offset and the state it starts in.
    pub(crate) fn next_with(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<Option<Lexeme>, Failure> {
        loop {
            if self.at_line_start {
                self.at_line_start = false;
                self.captured.clear();
                if let Some(lexeme) = self.line_start()? {
                    return Ok(Some(lexeme));
                }
                continue;
            }
            let Some(byte) = self.bytes.peek()? else {
                return Ok(None);
            };
            match byte {
                b'\n' => {
                    self.bytes.bump();
                    self.line += 1;
                    self.column = 1;
                    self.col = 0;
                    self.at_line_start = true;
                    if let Some(block) = self.next_block.take() {
                        self.state.block = Some(block);
                    }
                    on_line(self.line - 1, self.bytes.offset(), self.state);
                }
                b' ' | b'\t' | b'\r' => self.skip(),
                _ => {
                    self.captured.clear();
                    return self.token(byte).map(Some);
                }
            }
        }
    }

    /// The indentation of a new line, and the lexeme its state makes of it: a document marker, a
    /// line of a block or quoted scalar, a tab in the indentation.
    fn line_start(&mut self) -> Result<Option<Lexeme>, Failure> {
        self.node_col = None;
        self.scalar_col = None;
        self.adjacent = false;
        let begin = self.mark();
        let (mut indent, mut tab) = (0u32, false);
        while let Some(byte) = self.bytes.peek()? {
            match byte {
                b' ' if !tab => indent = indent.saturating_add(1),
                b' ' | b'\r' => {}
                b'\t' => tab = true,
                _ => break,
            }
            self.skip();
        }
        self.indent = indent;
        let Some(first) = self.bytes.peek()? else {
            return Ok(None);
        };
        if first == b'\n' {
            return Ok(None);
        }
        let line = self.line;
        if self.col == 0 && matches!(first, b'-' | b'.') && self.is_marker(first)? {
            self.state = LineState::default();
            self.next_block = None;
            let start = self.mark();
            for _ in 0..3 {
                self.take(first);
            }
            let kind = if first == b'-' {
                Kind::DocStart
            } else {
                Kind::DocEnd
            };
            return Ok(Some(self.lexeme(kind, start, line, self.bytes.offset())));
        }
        if let Some(block) = self.state.block {
            let needed = block.indent.unwrap_or(block.min);
            if indent >= needed {
                if block.indent.is_none() {
                    self.state.block = Some(Block {
                        indent: Some(indent),
                        ..block
                    });
                }
                let start = self.mark();
                let end = self.take_line_end()?;
                return Ok(Some(self.lexeme(Kind::BlockText, start, line, end)));
            }
            self.state.block = None;
        }
        if self.state.quote != 0 {
            let start = self.mark();
            let (kind, end) = self.quoted(self.state.quote, true)?;
            return Ok(Some(self.lexeme(kind, start, line, end)));
        }
        if tab && self.state.flow == 0 && first != b'#' {
            let end = self.bytes.offset();
            return Ok(Some(self.lexeme(
                Kind::Invalid(Problem::TabIndent),
                begin,
                line,
                end,
            )));
        }
        Ok(None)
    }

    /// Whether the line goes on with `---` or `...` and a blank or its end.
    fn is_marker(&mut self, byte: u8) -> Result<bool, Failure> {
        let mark = self.mark();
        let mut taken = 0;
        while taken < 3 && self.bytes.peek()? == Some(byte) {
            self.take(byte);
            taken += 1;
        }
        let next = self.bytes.peek()?;
        self.restore(mark);
        Ok(taken == 3 && ends_indicator(next, false))
    }

    /// Whether `byte` (`-`, `?`, `:`) is an indicator here: a blank or the end follows it.
    fn indicator(&mut self, byte: u8) -> Result<bool, Failure> {
        let mark = self.mark();
        self.take(byte);
        let next = self.bytes.peek()?;
        self.restore(mark);
        Ok(ends_indicator(next, self.state.flow > 0))
    }

    fn token(&mut self, byte: u8) -> Result<Lexeme, Failure> {
        let start = self.mark();
        let (line, col) = (self.line, self.col);
        let adjacent = std::mem::take(&mut self.adjacent);
        let flow = self.state.flow > 0;
        let mut end = None;
        let kind = match byte {
            b'#' => {
                end = Some(self.take_line_end()?);
                Kind::Comment
            }
            b'-' if !flow && self.indicator(byte)? => {
                self.take(byte);
                self.node_col = Some(col);
                Kind::Dash
            }
            b'?' if self.indicator(byte)? => {
                self.take(byte);
                Kind::Question
            }
            b':' if adjacent || self.indicator(byte)? => {
                self.take(byte);
                if !flow {
                    self.node_col = self.scalar_col.or(self.node_col);
                }
                Kind::Colon
            }
            b'[' | b'{' => {
                self.take(byte);
                self.state.flow = self.state.flow.saturating_add(1);
                Kind::FlowOpen
            }
            b']' | b'}' => {
                self.take(byte);
                if flow {
                    self.state.flow -= 1;
                    Kind::FlowClose
                } else {
                    Kind::Invalid(Problem::StrayClose)
                }
            }
            b',' if flow => {
                self.take(byte);
                Kind::Comma
            }
            b'"' | b'\'' => {
                self.scalar_col = Some(col);
                let (kind, quoted_end) = self.quoted(byte, false)?;
                end = Some(quoted_end);
                self.adjacent = flow && self.state.quote == 0;
                kind
            }
            b'&' | b'*' | b'!' => {
                self.take(byte);
                self.word()?;
                match byte {
                    b'&' => Kind::Anchor,
                    b'*' => {
                        self.scalar_col = Some(col);
                        Kind::Alias
                    }
                    _ => Kind::Tag,
                }
            }
            b'|' | b'>' if !flow => {
                self.block_header(byte)?;
                Kind::BlockHeader
            }
            b'|' | b'>' => {
                self.take(byte);
                self.word()?;
                Kind::Invalid(Problem::BlockInFlow)
            }
            b'%' if col == 0 => {
                end = Some(self.take_line_end()?);
                Kind::Directive
            }
            b'@' | b'`' => {
                self.take(byte);
                self.word()?;
                Kind::Invalid(Problem::Reserved)
            }
            _ => {
                self.scalar_col = Some(col);
                end = Some(self.plain()?);
                Kind::Plain
            }
        };
        let end = end.unwrap_or(self.bytes.offset());
        Ok(self.lexeme(kind, start, line, end))
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
