//! The script lexer: one line at a time, in the state the line before left, for both passes (the
//! line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the
//! Lister shows, so every offset is a byte of it.
//!
//! Each language has its own lexer ([`ruby`], [`perl`], [`lua`], [`vbs`]) over the shared
//! [`Cursor`] and the shared pieces of [`scan`] (names, numbers, operators) and [`quoted`]
//! (strings with their delimiters, nesting, interpolation and flags). What reaches from one line
//! into the next is one [`Block`]: a string over lines, a heredoc's body, Ruby's `=begin` or Perl's
//! POD, a Lua long bracket, the data after `__END__`.
//!
//! The lexers colour, they do not parse, and never fail: a file that is not well formed is
//! coloured as far as it reads. They never go back: every step moves on, and what they look ahead
//! at is bounded (an interpolation's closing brace on its own line, a heredoc's tag of at most
//! [`MAX_TAG`] bytes, an interpolation nested at most [`MAX_NEST`] deep), so the time is linear in
//! the line. Every decision is on ASCII bytes, so every token sits on character boundaries.

mod lua;
mod perl;
mod quoted;
mod ruby;
mod scan;
mod vbs;

pub(crate) use scan::Cursor;

use crate::lang::Lang;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The most strings remembered open at once for `invalid` at the end of the file.
const MAX_OPENS: usize = 64;
/// The most strings remembered that a line ended without closing them.
const MAX_FAILED: usize = 1024;
/// The longest heredoc tag kept; a longer one opens no heredoc, so a body is never swallowed by a
/// tag that cannot be matched.
pub(crate) const MAX_TAG: usize = 32;
/// The deepest interpolation lexed as code (`"#{"#{x}"}"`); a deeper one stays string text.
pub(crate) const MAX_NEST: u8 = 4;

/// The tag that ends a heredoc, `EOS` of `<<~EOS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tag {
    bytes: [u8; MAX_TAG],
    len: u8,
}

impl Tag {
    /// The tag `name`; `None` when it is empty or longer than [`MAX_TAG`].
    pub(crate) fn new(name: &[u8]) -> Option<Self> {
        if name.is_empty() || name.len() > MAX_TAG {
            return None;
        }
        let mut bytes = [0; MAX_TAG];
        bytes[..name.len()].copy_from_slice(name);
        Some(Self {
            bytes,
            len: u8::try_from(name.len()).ok()?,
        })
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// A string whose delimiters the lexer is between.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Quote {
    /// The opening delimiter: the closing one, unless it is a bracket, which nests.
    pub(crate) open: u8,
    pub(crate) close: u8,
    /// Brackets opened inside and not yet closed.
    pub(crate) depth: u8,
    /// `#{...}` (Ruby) or `$name` (Perl) are code, not text.
    pub(crate) interp: bool,
    /// The parts still to come after this one: Perl's `s/a/b/` and `tr/a/b/` have one.
    pub(crate) more: u8,
    /// Letters after the closing delimiter belong to it: a regular expression's flags.
    pub(crate) flags: bool,
}

/// A heredoc whose body the lines are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Heredoc {
    pub(crate) tag: Tag,
    /// `<<~` and `<<-`: the tag may stand indented.
    pub(crate) indented: bool,
    pub(crate) interp: bool,
}

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Block {
    /// Code.
    #[default]
    Code,
    /// Ruby's `=begin` ... `=end`, Perl's POD up to `=cut`; `data`: the POD stands after
    /// `__END__`, where `=cut` returns to.
    Doc { data: bool },
    /// After `__END__` or `__DATA__`: data, not code.
    Data,
    /// A Lua long bracket, `[==[` of `level` 2: a string, or a comment after `--`.
    Long { level: u8, comment: bool },
    /// A string over lines.
    Str(Quote),
    /// The body of a heredoc.
    Heredoc(Heredoc),
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) block: Block,
}

/// Where a string was opened that the file leaves open, or that its line ended without closing:
/// its opening (`"`, `%w[`, `<<~EOS`, `[==[`) shows as `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuoteOpen {
    pub(crate) line: u64,
    pub(crate) col: u32,
    pub(crate) width: u8,
}

/// What the lexer reports besides the state: tokens, and the strings it opens and closes.
pub(crate) trait Emit {
    /// The token `start..end` of the line.
    fn token(&mut self, _start: usize, _end: usize, _class: TokenClass) {}
    /// A string opens at byte `col`, its opening `width` bytes wide.
    fn opened(&mut self, _col: usize, _width: u8) {}
    /// The innermost string open closes.
    fn closed(&mut self) {}
    /// The innermost string open ends with its line, unclosed: the language refuses it.
    fn failed(&mut self) {}
}

/// Only the state.
impl Emit for () {}

/// The tokens of one line, at most [`MAX_TOKENS_PER_LINE`].
impl Emit for Vec<Token> {
    fn token(&mut self, start: usize, end: usize, class: TokenClass) {
        let (Ok(at), Ok(len)) = (
            u32::try_from(start),
            u32::try_from(end.saturating_sub(start)),
        ) else {
            return;
        };
        if len > 0 && self.len() < MAX_TOKENS_PER_LINE {
            self.push(Token {
                start: at,
                len,
                class,
            });
        }
    }
}

/// `open`'s pass: the strings open at each line's end, where they were opened, and the strings a
/// line ended without closing.
#[derive(Debug, Default)]
pub(crate) struct Opens {
    /// The line being lexed, 0-based; the caller moves it on.
    pub(crate) line: u64,
    open: Vec<QuoteOpen>,
    /// Strings open beyond `open`'s cap.
    lost: u64,
    failed: Vec<QuoteOpen>,
}

impl Opens {
    /// The strings to show as `invalid`: those a line ended unclosed and those still open at the
    /// file's end, in the order of the file.
    pub(crate) fn into_unclosed(self) -> Vec<QuoteOpen> {
        let mut all = self.failed;
        all.extend(self.open);
        all.sort_by_key(|open| (open.line, open.col));
        all.shrink_to_fit();
        all
    }

    /// Takes the innermost string open off the stack.
    fn pop(&mut self) -> Option<QuoteOpen> {
        if self.lost > 0 {
            self.lost -= 1;
            None
        } else {
            self.open.pop()
        }
    }
}

impl Emit for Opens {
    fn opened(&mut self, col: usize, width: u8) {
        match u32::try_from(col) {
            Ok(col) if self.lost == 0 && self.open.len() < MAX_OPENS => {
                self.open.push(QuoteOpen {
                    line: self.line,
                    col,
                    width,
                });
            }
            _ => self.lost += 1,
        }
    }

    fn closed(&mut self) {
        self.pop();
    }

    fn failed(&mut self) {
        if let Some(open) = self.pop()
            && self.failed.len() < MAX_FAILED
        {
            self.failed.push(open);
        }
    }
}

/// Lexes `line` of a `lang` file (UTF-8, without its line break and a `\r` before it) from
/// `state`, reporting to `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(
    lang: Lang,
    line: &[u8],
    state: LineState,
    emit: &mut impl Emit,
) -> LineState {
    let block = match lang {
        Lang::Ruby => ruby::line(line, state.block, emit),
        Lang::Perl => perl::line(line, state.block, emit),
        Lang::Lua => lua::line(line, state.block, emit),
        Lang::VbScript => {
            vbs::line(line, emit);
            Block::Code
        }
    };
    LineState { block }
}

/// Whether `line`, less its leading blanks when `indented`, is `tag` - the line that ends a
/// heredoc.
pub(crate) fn ends_heredoc(line: &[u8], heredoc: &Heredoc) -> bool {
    let line = if heredoc.indented {
        line.trim_ascii_start()
    } else {
        line
    };
    line == heredoc.tag.bytes()
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
