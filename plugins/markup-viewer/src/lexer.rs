//! The markup lexer: one line at a time, in the state the line before left and with the next
//! line's text beside it, for both passes (the line index of `open` and the tokens of the lines
//! on screen). It reads the UTF-8 text the Lister shows, so every offset is a byte of it.
//!
//! Each format has its own lexer ([`rst`] with [`inline`], [`latex`], [`bibtex`], [`todo`]) over
//! the shared [`Cursor`]. What reaches from one line into the next is one [`Block`] - an indented
//! reStructuredText block or the expectation of one, LaTeX math or a verbatim environment, a
//! BibTeX entry and its value - and whether the line is a title's underline.
//!
//! The lexers colour, they do not parse, and never fail: a file that is not well formed is
//! coloured as far as it reads. They never go back: every step moves on, and what they look ahead
//! at is bounded (the next line, an inline end-string within [`MAX_INLINE`] bytes, an
//! environment's name of at most [`MAX_TAG`] bytes), so the time is linear in the line. Every
//! decision is on ASCII bytes, so every token sits on character boundaries.

mod bibtex;
mod cursor;
mod inline;
mod latex;
mod rst;
mod todo;

pub(crate) use cursor::Cursor;

use crate::lang::Lang;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The most strings remembered open at once for `invalid` at the end of the file.
const MAX_OPENS: usize = 64;
/// The most strings remembered that a line ended without closing them.
const MAX_FAILED: usize = 1024;
/// The longest environment name kept for a verbatim environment's end.
pub(crate) const MAX_TAG: usize = 32;
/// The most bytes an inline end-string (`**`, `` ` ``, `|`) is looked for after its start.
pub(crate) const MAX_INLINE: usize = 4096;

/// The name that ends a verbatim environment, `verbatim` of `\end{verbatim}`.
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

/// What a reStructuredText indented block holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Body {
    /// After `::`: literal text.
    #[default]
    Literal,
    /// After `..` that is no directive, target or footnote: a comment.
    Comment,
    /// A directive's or footnote's content: options, then text with inline markup.
    Directive,
    /// The content of `code`, `code-block`, `sourcecode`, `math`, `raw`: options, then literal.
    Code,
}

/// What closes LaTeX math.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MathClose {
    /// `$`.
    Dollar,
    /// `$$`.
    Dollars,
    /// `\)`.
    Paren,
    /// `\]`.
    Bracket,
}

/// A BibTeX value still open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Value {
    #[default]
    None,
    /// `{...}`, with the braces open inside it.
    Braced(u8),
    /// `"..."`, with the braces open inside it.
    Quoted(u8),
}

/// A BibTeX entry being read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Entry {
    /// `}` or `)`, after the opening the entry took.
    pub(crate) close: u8,
    /// The citation key comes next.
    pub(crate) key: bool,
    pub(crate) value: Value,
}

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Block {
    /// Text.
    #[default]
    Text,
    /// reStructuredText: lines indented deeper than `indent`, or blank, belong to the block.
    Indented { indent: u16, body: Body },
    /// reStructuredText: after a paragraph ending in `::`, a literal block when the next line
    /// that is not blank is indented deeper than `indent`.
    Expect { indent: u16 },
    /// LaTeX math.
    Math(MathClose),
    /// LaTeX: a verbatim environment, up to `\end{tag}`; `comment`: the `comment` environment.
    Verbatim { tag: Tag, comment: bool },
    /// BibTeX: an entry.
    Entry(Entry),
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) block: Block,
    /// reStructuredText: the line underlines the title before it.
    pub(crate) underline: bool,
}

/// Where something was opened that the file leaves open, or that a line ended without closing:
/// its opening (`$`, `\[`, `\begin`, `{`, `"`) shows as `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuoteOpen {
    pub(crate) line: u64,
    pub(crate) col: u32,
    pub(crate) width: u8,
}

/// What the lexer reports besides the state: tokens, and what it opens and closes.
pub(crate) trait Emit {
    /// The token `start..end` of the line.
    fn token(&mut self, _start: usize, _end: usize, _class: TokenClass) {}
    /// Something opens at byte `col`, its opening `width` bytes wide.
    fn opened(&mut self, _col: usize, _width: u8) {}
    /// The innermost thing open closes.
    fn closed(&mut self) {}
    /// The innermost thing open ends unclosed: a blank line ended LaTeX math.
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

/// `open`'s pass: what is open at each line's end, where it was opened, and what a line ended
/// without closing.
#[derive(Debug, Default)]
pub(crate) struct Opens {
    /// The line being lexed, 0-based; the caller moves it on.
    pub(crate) line: u64,
    open: Vec<QuoteOpen>,
    /// Openings beyond `open`'s cap.
    lost: u64,
    failed: Vec<QuoteOpen>,
}

impl Opens {
    /// What to show as `invalid`: what a line ended unclosed and what is still open at the file's
    /// end, in the order of the file.
    pub(crate) fn into_unclosed(self) -> Vec<QuoteOpen> {
        let mut all = self.failed;
        all.extend(self.open);
        all.sort_by_key(|open| (open.line, open.col));
        all.shrink_to_fit();
        all
    }

    /// Takes the innermost opening off the stack.
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
/// `state`, `next` the line after it (`None` at the file's end), reporting to `emit`. Returns the
/// state the next line starts in.
pub(crate) fn lex_line(
    lang: Lang,
    line: &[u8],
    next: Option<&[u8]>,
    state: LineState,
    emit: &mut impl Emit,
) -> LineState {
    let mut cur = Cursor::new(line, emit);
    match lang {
        Lang::Rst => rst::line(&mut cur, next, state),
        Lang::Latex => LineState {
            block: latex::line(&mut cur, state.block),
            underline: false,
        },
        Lang::Bibtex => LineState {
            block: bibtex::line(&mut cur, state.block),
            underline: false,
        },
        Lang::Todo => {
            todo::line(&mut cur);
            LineState::default()
        }
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
