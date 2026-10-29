//! Quotes and expansions: `'...'`, `$'...'`, `"..."` with the expansions coloured inside it,
//! `$name`, `${...}`, `$(`, `$((` and arithmetic.

use super::state::{Frame, Pos, Quote};
use super::words;
use super::{Ctx, Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

impl<S: Source> Lexer<'_, S> {
    /// `"` or `$"` (`width` 1 or 2) opens a double-quoted string.
    pub(super) fn open_dquote(&mut self, start: u32, width: u8) -> Result<Lexeme, Failure> {
        self.take_n(usize::from(width))?;
        self.state.frames.push(Frame::DQuote);
        self.quote_opened(start, width);
        self.dquote_text(start)
    }

    /// The lexeme at `byte` inside a double-quoted string.
    pub(super) fn in_dquote(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        match byte {
            b'$' if self.expands(Ctx::String)? => self.dollar(start, Ctx::String),
            b'`' => self.backtick(start, Ctx::String),
            _ => self.dquote_text(start),
        }
    }

    /// A double-quoted string's text, up to its closing `"`, an expansion, a backtick or the
    /// line's end.
    fn dquote_text(&mut self, start: u32) -> Result<Lexeme, Failure> {
        while !self.full(start) && !self.at_line_end()? {
            match self.peek()? {
                Some(b'"') => {
                    self.take()?;
                    self.state.frames.pop();
                    self.quote_closed();
                    break;
                }
                Some(b'`') => break,
                Some(b'$') if self.expands(Ctx::String)? => break,
                Some(b'\\') => self.escape()?,
                _ => {
                    self.take()?;
                }
            }
        }
        Ok(self.lexeme(Some(TokenClass::String), start))
    }

    /// `'` or `$'` (`width` 1 or 2) opens a single-quoted string.
    pub(super) fn open_single(
        &mut self,
        quote: Quote,
        start: u32,
        width: u8,
    ) -> Result<Lexeme, Failure> {
        self.take_n(usize::from(width))?;
        self.state.quote = quote;
        self.quote_opened(start, width);
        self.single_text(start)
    }

    /// A single-quoted string's text, up to its closing `'` or the line's end; in `$'...'` a `\`
    /// escapes the character after it.
    pub(super) fn single_text(&mut self, start: u32) -> Result<Lexeme, Failure> {
        while !self.full(start) && !self.at_line_end()? {
            match self.peek()? {
                Some(b'\'') => {
                    self.take()?;
                    self.state.quote = Quote::None;
                    self.quote_closed();
                    break;
                }
                Some(b'\\') if self.state.quote == Quote::Dollar => self.escape()?,
                _ => {
                    self.take()?;
                }
            }
        }
        Ok(self.lexeme(Some(TokenClass::String), start))
    }

    /// Whether the `$` next expands in `ctx`: `$'` and `$"` only outside a double quote.
    pub(super) fn expands(&mut self, ctx: Ctx) -> Result<bool, Failure> {
        Ok(match self.peek_at(1)? {
            Some(b'\'' | b'"') => ctx != Ctx::String,
            Some(byte) => words::expands(byte),
            None => false,
        })
    }

    /// A `$`: `$((`, `$(`, `${`, `$'`, `$"`, `$name`, `$1`, `$@` ... or a `$` that is just one.
    pub(super) fn dollar(&mut self, start: u32, ctx: Ctx) -> Result<Lexeme, Failure> {
        if ctx == Ctx::Command {
            self.start_word();
        }
        let quoted = ctx != Ctx::String;
        match self.peek_at(1)? {
            Some(b'(') => {
                self.take_n(2)?;
                if self.peek()? == Some(b'(') {
                    self.take()?;
                    self.state.frames.push(Frame::Arith);
                } else {
                    self.state.frames.push(Frame::Subst);
                    self.state.pos = Pos::Command;
                    self.end_word();
                }
                Ok(self.lexeme(Some(TokenClass::Punctuation), start))
            }
            Some(b'{') => {
                self.take_n(2)?;
                self.state.frames.push(Frame::Param);
                self.param_text(start)
            }
            Some(b'\'') if quoted => self.open_single(Quote::Dollar, start, 2),
            Some(b'"') if quoted => self.open_dquote(start, 2),
            Some(byte) if words::name_start(byte) => {
                self.take()?;
                while self.peek()?.is_some_and(words::name_byte) && !self.full(start) {
                    self.take()?;
                }
                Ok(self.lexeme(Some(TokenClass::Key), start))
            }
            Some(byte) if words::expands(byte) => {
                self.take_n(2)?;
                Ok(self.lexeme(Some(TokenClass::Key), start))
            }
            _ => {
                self.take()?;
                let class = match ctx {
                    Ctx::String => Some(TokenClass::String),
                    Ctx::Param => Some(TokenClass::Key),
                    Ctx::Command | Ctx::Arith => None,
                };
                Ok(self.lexeme(class, start))
            }
        }
    }

    /// The lexeme at `byte` inside `${...}`.
    pub(super) fn in_param(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        match byte {
            b'"' => self.open_dquote(start, 1),
            b'\'' => self.open_single(Quote::Single, start, 1),
            b'$' if self.expands(Ctx::Param)? => self.dollar(start, Ctx::Param),
            b'`' => self.backtick(start, Ctx::Param),
            _ => self.param_text(start),
        }
    }

    /// The text of `${...}` as `key`, its `}` included: up to a quote, an expansion, a backtick
    /// or the line's end.
    fn param_text(&mut self, start: u32) -> Result<Lexeme, Failure> {
        while !self.full(start) && !self.at_line_end()? {
            match self.peek()? {
                Some(b'}') => {
                    self.take()?;
                    self.state.frames.pop();
                    break;
                }
                Some(b'"' | b'\'' | b'`') => break,
                Some(b'$') if self.expands(Ctx::Param)? => break,
                Some(b'\\') => self.escape()?,
                _ => {
                    self.take()?;
                }
            }
        }
        Ok(self.lexeme(Some(TokenClass::Key), start))
    }

    /// The lexeme at `byte` inside `$((...))` or `((...))`: numbers, names as `key`, operators,
    /// parentheses.
    pub(super) fn in_arith(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        let class = match byte {
            b' ' | b'\t' | b'\r' => return self.blanks(start),
            b'$' if self.expands(Ctx::Arith)? => return self.dollar(start, Ctx::Arith),
            b'"' => return self.open_dquote(start, 1),
            b'(' => {
                self.take()?;
                self.state.frames.push(Frame::ArithParen);
                Some(TokenClass::Punctuation)
            }
            b')' => {
                self.take()?;
                if self.state.frames.top() == Some(Frame::Arith) {
                    if self.peek()? == Some(b')') {
                        self.take()?;
                    }
                    self.state.pos = Pos::Args;
                    self.in_word = true;
                }
                self.state.frames.pop();
                Some(TokenClass::Punctuation)
            }
            b'0'..=b'9' => {
                // `42`, `0x1f`, `16#ff`, `1.5`
                while self
                    .peek()?
                    .is_some_and(|byte| words::name_byte(byte) || matches!(byte, b'#' | b'.'))
                    && !self.full(start)
                {
                    self.take()?;
                }
                Some(TokenClass::Number)
            }
            byte if words::name_start(byte) => {
                while self.peek()?.is_some_and(words::name_byte) && !self.full(start) {
                    self.take()?;
                }
                Some(TokenClass::Key)
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^'
            | b'~' | b'?' | b':' | b',' => {
                while self.peek()?.is_some_and(is_arith_operator) && !self.full(start) {
                    self.take()?;
                }
                Some(TokenClass::Operator)
            }
            _ => {
                self.take()?;
                None
            }
        };
        Ok(self.lexeme(class, start))
    }
}

/// Whether `byte` is part of an arithmetic operator.
fn is_arith_operator(byte: u8) -> bool {
    b"+-*/%=<>!&|^~?:,".contains(&byte)
}
