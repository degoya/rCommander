//! The shell lexer, one for both passes: the line index of `open` and the tokens of the lines on
//! screen.
//!
//! A lexeme never spans a line, and none is longer than [`MAX_PIECE`] bytes: a longer string,
//! word or comment comes in pieces. What a shell script carries from one line into the next is
//! the [`LineState`]: the open frames (quotes, `$(`, `${`, `$((`, backticks, `case`), an open
//! single quote, a heredoc and its marker, where the next word stands when a `\` continued the
//! line. The line index keeps that state at every checkpoint, so `tokens` starts at any of them
//! without reading what lies before. Columns count the text written: UTF-8, U+FFFD for a byte
//! that is not ([`crate::utf8`]).
//!
//! The lexer colours, it does not parse: it knows the shell's words well enough to tell a
//! keyword from an argument, and never fails - a script that is not well formed is coloured as
//! far as it reads.

use crate::reader::Reader;
use crate::types::{Failure, Source, TokenClass};
use crate::utf8;

mod command;
mod heredoc;
mod quoted;
mod state;
pub(crate) mod words;

pub(crate) use state::LineState;
use state::{Frame, Heredoc, Pos, Quote};

/// The longest lexeme, in bytes of the text.
pub(crate) const MAX_PIECE: u32 = 4096;
/// The most quote openings remembered for `invalid` at the end of the file.
const MAX_OPENS: usize = 64;

/// One lexeme: `len` bytes of the text from byte column `col` of 0-based line `line`. `class`
/// `None` for what is not coloured: blanks, plain words, a line break (`len` 0, `line` the line
/// it starts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme {
    pub(crate) class: Option<TokenClass>,
    pub(crate) line: u64,
    pub(crate) col: u32,
    pub(crate) len: u32,
}

/// Where a quote the file leaves open was opened, and how wide its opening is (`"` 1, `$"` 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuoteOpen {
    pub(crate) line: u64,
    pub(crate) col: u32,
    pub(crate) width: u8,
}

/// Where a piece is read: what a `$` or a quote means there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    Command,
    String,
    Param,
    Arith,
}

/// What a word is, decided when it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// The first piece of a word standing at `Pos`.
    At(Pos),
    /// A redirection's target.
    Target,
    /// A `case` pattern.
    Pattern,
    /// The value after an assignment's `=`.
    Value,
    /// A further piece of a word already started.
    Inner,
}

pub(crate) struct Lexer<'a, S: Source> {
    reader: Reader<'a, S>,
    /// The 0-based line of the next byte.
    line: u64,
    /// The byte column of the next character in the text.
    col: u32,
    state: LineState,
    at_line_start: bool,
    /// Inside a word: a `#` starts no comment, and the next piece goes on with the word.
    in_word: bool,
    /// The rest of a comment longer than one piece.
    in_comment: bool,
    /// A heredoc line that is not its marker.
    in_body: bool,
    /// A `\` ends the line: the next one goes on with the same command.
    continued: bool,
    /// The next word is a redirection's target.
    redirect: bool,
    /// After `<<` (`Some(false)`) or `<<-` (`Some(true)`): the next word is a heredoc's marker.
    marker: Option<bool>,
    /// The heredoc whose marker this line named; its text starts on the next line.
    pending: Option<Heredoc>,
    /// The word after an assignment's `=`.
    value: bool,
    /// A lexeme found while reading another, handed out next.
    queued: Option<Lexeme>,
    /// The quotes open now, where they were opened; only a lexer from the file's start knows.
    opens: Vec<QuoteOpen>,
    /// Quotes open beyond `opens`' cap.
    lost: u32,
}

impl<'a, S: Source> Lexer<'a, S> {
    /// A lexer at the start of the text.
    pub(crate) fn new(reader: Reader<'a, S>) -> Self {
        Self::resume(reader, 0, LineState::default())
    }

    /// A lexer at the start of 0-based line `line`, which starts in `state`.
    pub(crate) fn resume(reader: Reader<'a, S>, line: u64, state: LineState) -> Self {
        Self {
            reader,
            line,
            col: 0,
            state,
            at_line_start: true,
            in_word: false,
            in_comment: false,
            in_body: false,
            continued: false,
            redirect: false,
            marker: None,
            pending: None,
            value: false,
            queued: None,
            opens: Vec::new(),
            lost: 0,
        }
    }

    /// Goes on at `offset`, the start of 0-based line `line` in `state`.
    pub(crate) fn seek_line(&mut self, offset: u64, line: u64, state: LineState) {
        self.reader.seek(offset);
        self.line = line;
        self.col = 0;
        self.state = state;
        self.at_line_start = true;
        self.reset_line();
        self.continued = false;
        self.pending = None;
        self.queued = None;
    }

    /// The lines read: a last line without a line break counts, an empty one after the last
    /// break does not.
    pub(crate) fn line_count(&self) -> u64 {
        self.line + u64::from(self.col > 0)
    }

    /// The quotes still open, where they were opened.
    pub(crate) fn open_quotes(&self) -> &[QuoteOpen] {
        &self.opens
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
        if let Some(lexeme) = self.queued.take() {
            return Ok(Some(lexeme));
        }
        if std::mem::take(&mut self.at_line_start)
            && self.state.heredoc.is_some()
            && let Some(lexeme) = self.body_start()?
        {
            return Ok(Some(lexeme));
        }
        let Some(byte) = self.reader.peek()? else {
            return Ok(None);
        };
        let start = self.col;
        if byte == b'\n' {
            self.newline(on_line);
            return Ok(Some(self.lexeme(None, 0)));
        }
        if byte == b'\r' && self.reader.peek_at(1)? == Some(b'\n') {
            self.take()?;
            return Ok(Some(self.lexeme(None, start)));
        }
        let lexeme = if self.in_comment {
            self.comment(start)?
        } else if self.in_body {
            self.body(start)?
        } else if self.state.quote != Quote::None {
            self.single_text(start)?
        } else {
            match self.state.frames.top() {
                Some(Frame::DQuote) => self.in_dquote(byte, start)?,
                Some(Frame::Param) => self.in_param(byte, start)?,
                Some(Frame::Arith | Frame::ArithParen) => self.in_arith(byte, start)?,
                _ => self.in_command(byte, start)?,
            }
        };
        Ok(Some(lexeme))
    }

    /// Takes the line break; the next line starts.
    fn newline(&mut self, on_line: &mut dyn FnMut(u64, u64, LineState)) {
        self.reader.bump(1);
        self.line += 1;
        self.col = 0;
        self.at_line_start = true;
        self.reset_line();
        if let Some(heredoc) = self.pending.take() {
            self.state.heredoc = Some(heredoc);
        }
        let commands = self.state.quote == Quote::None
            && !matches!(
                self.state.frames.top(),
                Some(Frame::DQuote | Frame::Param | Frame::Arith | Frame::ArithParen)
            );
        if !std::mem::take(&mut self.continued) && commands {
            self.state.pos = Pos::Command;
        }
        on_line(self.line, self.reader.offset(), self.state);
    }

    /// Forgets what holds only within a line.
    fn reset_line(&mut self) {
        self.end_word();
        self.in_comment = false;
        self.in_body = false;
        self.redirect = false;
        self.marker = None;
    }

    /// Takes one character; its first byte (`0x80` or more for anything but ASCII), `None` at
    /// the end.
    fn take(&mut self) -> Result<Option<u8>, Failure> {
        if let Some(byte) = self.reader.peek()?
            && byte < 0x80
        {
            self.reader.bump(1);
            self.col = self.col.saturating_add(1);
            return Ok(Some(byte));
        }
        let bytes = self.reader.look(4)?;
        let Some(&first) = bytes.first() else {
            return Ok(None);
        };
        let (raw, out) = utf8::unit(bytes);
        self.reader.bump(raw);
        self.col = self.col.saturating_add(out);
        Ok(Some(first))
    }

    /// Takes `count` characters.
    fn take_n(&mut self, count: usize) -> Result<(), Failure> {
        for _ in 0..count {
            self.take()?;
        }
        Ok(())
    }

    /// Takes characters up to the source offset `end`.
    fn take_to(&mut self, end: u64) -> Result<(), Failure> {
        while self.reader.offset() < end && self.take()?.is_some() {}
        Ok(())
    }

    fn peek(&mut self) -> Result<Option<u8>, Failure> {
        self.reader.peek()
    }

    fn peek_at(&mut self, ahead: usize) -> Result<Option<u8>, Failure> {
        self.reader.peek_at(ahead)
    }

    /// Whether the line ends here: a line break, `\r\n` or the end of the text.
    fn at_line_end(&mut self) -> Result<bool, Failure> {
        Ok(match self.peek()? {
            None | Some(b'\n') => true,
            Some(b'\r') => self.peek_at(1)? == Some(b'\n'),
            Some(_) => false,
        })
    }

    /// Whether the `\` next is the last character of its line: a line continuation.
    fn continues(&mut self) -> Result<bool, Failure> {
        Ok(match self.peek_at(1)? {
            Some(b'\n') => true,
            Some(b'\r') => self.peek_at(2)? == Some(b'\n'),
            _ => false,
        })
    }

    /// Takes a `\` and the character it escapes, if that is on the same line.
    fn escape(&mut self) -> Result<(), Failure> {
        self.take()?;
        if !self.at_line_end()? {
            self.take()?;
        }
        Ok(())
    }

    /// Whether the piece begun at `start` is as long as a piece gets.
    fn full(&self, start: u32) -> bool {
        self.col.saturating_sub(start) >= MAX_PIECE
    }

    /// The lexeme from `start` to here.
    fn lexeme(&self, class: Option<TokenClass>, start: u32) -> Lexeme {
        Lexeme {
            class,
            line: self.line,
            col: start,
            len: self.col.saturating_sub(start),
        }
    }

    /// What the word a piece belongs to is; a new word moves the position on.
    fn start_word(&mut self) -> Role {
        if self.in_word {
            return if std::mem::take(&mut self.value) {
                Role::Value
            } else {
                Role::Inner
            };
        }
        self.in_word = true;
        if std::mem::take(&mut self.redirect) {
            return Role::Target;
        }
        if self.state.frames.top() == Some(Frame::CasePattern) {
            return Role::Pattern;
        }
        let pos = self.state.pos;
        self.state.pos = match pos {
            Pos::Decl => Pos::Decl,
            Pos::ForVar => Pos::ForIn,
            Pos::CaseWord => Pos::CaseIn,
            _ => Pos::Args,
        };
        Role::At(pos)
    }

    fn end_word(&mut self) {
        self.in_word = false;
        self.value = false;
    }

    /// A quote opened at `col`, `width` bytes wide.
    fn quote_opened(&mut self, col: u32, width: u8) {
        if self.opens.len() < MAX_OPENS && self.lost == 0 {
            self.opens.push(QuoteOpen {
                line: self.line,
                col,
                width,
            });
        } else {
            self.lost = self.lost.saturating_add(1);
        }
    }

    /// The quote opened last is closed.
    fn quote_closed(&mut self) {
        if self.lost > 0 {
            self.lost -= 1;
        } else {
            self.opens.pop();
        }
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
