//! The lexer of the web languages, one for both passes: the line index of `open` and the tokens
//! of the lines on screen.
//!
//! A lexeme never spans a line, and none is longer than [`MAX_PIECE`] bytes: a longer string or
//! comment comes in pieces. The languages nest, so what a line starts inside of is a stack of
//! frames ([`LineState`]): HTML at the bottom of an HTML, Vue or PHP file, CSS or JavaScript at
//! the bottom of theirs; `<style>` and `<script>` open CSS and JavaScript, a `style` or `on...`
//! attribute's value too, a template string's `${` opens JavaScript again, a JSX element its
//! tags and children; `<?php` runs PHP over all of it until `?>`. The line index keeps that
//! state at every checkpoint, so `tokens` starts at any of them without reading what lies
//! before. Columns count the text written: UTF-8, U+FFFD for a byte that is not
//! ([`crate::utf8`]).
//!
//! An embedded language ends where the browser ends it, not where its own grammar would:
//! `</script` closes a script inside a string or a comment, an attribute's quote closes the
//! JavaScript or CSS in it. Every scanner stops at such a boundary ([`Lexer::more`]), and the
//! frames above it close there.
//!
//! The lexer colours, it does not parse: it knows the languages well enough to tell a keyword
//! from a name and a regular expression from a division, and never fails - a file that is not
//! well formed is coloured as far as it reads.

use crate::reader::Reader;
use crate::types::{Failure, Source, TokenClass};
use crate::utf8;

mod css;
mod html;
mod js;
mod js_words;
mod jsx;
mod php;
mod scan;
pub(crate) mod state;
mod words;

pub(crate) use state::LineState;
use state::{End, Frame, PhpSub};

/// The longest lexeme, in bytes of the text.
pub(crate) const MAX_PIECE: u32 = 4096;
/// Lexemes in a row that took nothing before one character is taken as `invalid`: a guard, so
/// no state the lexer did not foresee can hold it in place.
const MAX_STALLS: u8 = 8;

/// One lexeme: `len` bytes of the text from byte column `col` of 0-based line `line`. `class`
/// `None` for what is not coloured: blanks, plain names and text, a line break (`len` 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme {
    pub(crate) class: Option<TokenClass>,
    pub(crate) line: u64,
    pub(crate) col: u32,
    pub(crate) len: u32,
}

/// What holds only within a line: forgotten at every line break, so a lexer started at a
/// checkpoint reads a line as one that read everything before it.
#[derive(Clone, Copy, Debug, Default)]
struct Scratch {
    /// The rest of the line is this class: a `//` comment longer than a piece, or `invalid`
    /// after a frame that could not be opened.
    rest: Option<TokenClass>,
    js: js::Scratch,
    css: css::Scratch,
    html: html::Scratch,
    php: php::Scratch,
}

pub(crate) struct Lexer<'a, S: Source> {
    reader: Reader<'a, S>,
    /// The 0-based line of the next byte.
    line: u64,
    /// The byte column of the next character in the text.
    col: u32,
    state: LineState,
    /// The file is PHP: `<?php` and `<?=` open PHP wherever PHP is not running.
    php_file: bool,
    /// The topmost frame a quote or a closing tag ends ([`state::Stack::boundary`]), kept as
    /// the stack changes.
    boundary: Option<(usize, End)>,
    scratch: Scratch,
    stalls: u8,
}

impl<'a, S: Source> Lexer<'a, S> {
    /// A lexer at the start of 0-based line `line`, which starts in `state`.
    pub(crate) fn resume(
        reader: Reader<'a, S>,
        line: u64,
        state: LineState,
        php_file: bool,
    ) -> Self {
        Self {
            reader,
            line,
            col: 0,
            boundary: state.stack.boundary(),
            state,
            php_file,
            scratch: Scratch::default(),
            stalls: 0,
        }
    }

    /// Goes on at `offset`, the start of 0-based line `line` in `state`.
    pub(crate) fn seek_line(&mut self, offset: u64, line: u64, state: LineState) {
        self.reader.seek(offset);
        self.line = line;
        self.col = 0;
        self.state = state;
        self.boundary = state.stack.boundary();
        self.scratch = Scratch::default();
        self.stalls = 0;
    }

    /// The lines read: a last line without a line break counts, an empty one after the last
    /// break does not.
    pub(crate) fn line_count(&self) -> u64 {
        self.line + u64::from(self.col > 0)
    }

    pub(crate) fn next(&mut self) -> Result<Option<Lexeme>, Failure> {
        self.next_with(&mut |_, _, _| {})
    }

    /// The next lexeme; `on_line` hears of every line started on the way: its 0-based number,
    /// its offset and the state it starts in.
    pub(crate) fn next_with(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64, &LineState),
    ) -> Result<Option<Lexeme>, Failure> {
        let Some(byte) = self.peek()? else {
            return Ok(None);
        };
        let start = self.col;
        if byte == b'\n' {
            self.newline(on_line);
            return Ok(Some(self.lexeme(None, 0)));
        }
        if byte == b'\r' && self.peek_at(1)? == Some(b'\n') {
            self.take()?;
            return Ok(Some(self.lexeme(None, start)));
        }
        let lexeme = self.dispatch(byte, start)?;
        if lexeme.len == 0 {
            self.stalls += 1;
            if self.stalls >= MAX_STALLS {
                self.stalls = 0;
                self.take()?;
                return Ok(Some(self.lexeme(Some(TokenClass::Invalid), start)));
            }
        } else {
            self.stalls = 0;
        }
        Ok(Some(lexeme))
    }

    /// The lexeme at `byte`, by the frame on top - after PHP opened or a boundary closed frames.
    fn dispatch(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        if let Some(sub) = self.state.php {
            return self.php(sub, byte, start);
        }
        if byte == b'<' && self.php_file {
            let open = self.php_open()?;
            if open > 0 {
                self.take_n(open)?;
                self.state.php = Some(PhpSub::Code);
                self.scratch.rest = None;
                return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
            }
        }
        if let Some((index, end)) = self.boundary
            && self.ends_here(end, byte)?
        {
            self.state.stack.truncate(index);
            self.changed();
        }
        if let Some(class) = self.scratch.rest {
            return self.rest_of_line(class, start);
        }
        match self.state.stack.top() {
            Frame::Html(sub) => self.html(sub, byte, start),
            Frame::Css(css) => self.css(css, byte, start),
            Frame::Js(js) => self.js(js, byte, start),
            Frame::Template(dialect) => self.template(dialect, start),
            Frame::JsxTag(dialect, sub) => self.jsx_tag(dialect, sub, byte, start),
            Frame::JsxChildren(dialect) => self.jsx_children(dialect, byte, start),
            Frame::JsxClose => self.jsx_close(byte, start),
            Frame::Raw => self.plain(start),
        }
    }

    /// Takes the line break; the next line starts.
    fn newline(&mut self, on_line: &mut dyn FnMut(u64, u64, &LineState)) {
        self.reader.bump(1);
        self.line += 1;
        self.col = 0;
        self.scratch = Scratch::default();
        self.stalls = 0;
        on_line(self.line, self.reader.offset(), &self.state);
    }

    /// Opens `frame` on top. At the deepest the stack goes it is not opened, and the rest of the
    /// line is `invalid`: the lexeme for that comes back.
    fn open(&mut self, frame: Frame, start: u32) -> Result<Option<Lexeme>, Failure> {
        if self.state.stack.push(frame) {
            self.changed();
            return Ok(None);
        }
        self.scratch.rest = Some(TokenClass::Invalid);
        self.rest_of_line(TokenClass::Invalid, start).map(Some)
    }

    /// Closes the frame on top.
    fn close(&mut self) {
        self.state.stack.pop();
        self.changed();
    }

    /// Replaces the frame on top: the same construct, gone on.
    fn set_top(&mut self, frame: Frame) {
        let ends = self.state.stack.top().end() != frame.end();
        self.state.stack.set_top(frame);
        if ends {
            self.changed();
        }
    }

    /// The stack changed: its boundary is found again, and what one frame noted for its next
    /// word means nothing to another.
    fn changed(&mut self) {
        self.boundary = self.state.stack.boundary();
        self.scratch.js = js::Scratch::default();
        self.scratch.rest = None;
    }

    /// Whether `end` ends its frame at `byte`: the frame's quote, or `</script` or `</style`
    /// followed by a blank, `>` or `/`, in any case.
    fn ends_here(&mut self, end: End, byte: u8) -> Result<bool, Failure> {
        let name: &[u8] = match end {
            End::Quote(quote) => return Ok(byte == quote),
            End::ScriptTag => b"</script",
            End::StyleTag => b"</style",
            End::Never | End::Brace => return Ok(false),
        };
        if byte != b'<' {
            return Ok(false);
        }
        let bytes = self.reader.look(name.len() + 1)?;
        let named = bytes
            .get(..name.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(name));
        let closed = bytes
            .get(name.len())
            .is_none_or(|after| after.is_ascii_whitespace() || matches!(after, b'>' | b'/'));
        Ok(named && closed)
    }

    /// The length of a PHP opening tag here: `<?php` (followed by no letter) 5, `<?=` 3, else 0.
    fn php_open(&mut self) -> Result<usize, Failure> {
        let bytes = self.reader.look(6)?;
        if bytes.starts_with(b"<?=") {
            return Ok(3);
        }
        let tag = bytes
            .get(..5)
            .is_some_and(|head| head.eq_ignore_ascii_case(b"<?php"));
        let alone = bytes.get(5).is_none_or(|after| !scan::is_ident(*after));
        Ok(if tag && alone { 5 } else { 0 })
    }

    /// Whether `byte`, the next one, is where a piece must stop for another language: a PHP
    /// opening tag, or the boundary of a frame below.
    fn at_boundary(&mut self, byte: u8) -> Result<bool, Failure> {
        if self.state.php.is_some() {
            return Ok(false);
        }
        if byte == b'<' && self.php_file && self.php_open()? > 0 {
            return Ok(true);
        }
        match self.boundary {
            Some((_, end)) => self.ends_here(end, byte),
            None => Ok(false),
        }
    }

    /// The next byte when the piece begun at `start` goes on with it: not at the line's end,
    /// not at a boundary, not when the piece is as long as a piece gets.
    fn more(&mut self, start: u32) -> Result<Option<u8>, Failure> {
        let Some(byte) = self.peek()? else {
            return Ok(None);
        };
        if byte == b'\n' || self.col.saturating_sub(start) >= MAX_PIECE {
            return Ok(None);
        }
        if byte == b'\r' && self.peek_at(1)? == Some(b'\n') {
            return Ok(None);
        }
        if self.at_boundary(byte)? {
            return Ok(None);
        }
        Ok(Some(byte))
    }

    /// Whether the line ends here: a line break, `\r\n` or the end of the text.
    fn at_line_end(&mut self) -> Result<bool, Failure> {
        Ok(match self.peek()? {
            None | Some(b'\n') => true,
            Some(b'\r') => self.peek_at(1)? == Some(b'\n'),
            Some(_) => false,
        })
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

    fn peek(&mut self) -> Result<Option<u8>, Failure> {
        self.reader.peek()
    }

    fn peek_at(&mut self, ahead: usize) -> Result<Option<u8>, Failure> {
        self.reader.peek_at(ahead)
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
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
