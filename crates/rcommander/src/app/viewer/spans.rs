//! A document's coloured text (RC-100-10): the token spans of the lines on screen, asked of the
//! source once per line and kept for the page, laid onto the text as the Lister shows it.
//!
//! The text stays the Lister's: the page is the same `Content` with or without colour, so wrap,
//! search, selection, copying and the text size work as before; the colours are an iced
//! highlighter over it ([`SpanHighlighter`]) that only paints. A token counts UTF-8 bytes within
//! its source line; the Lister shows the line with tabs expanded, control characters as dots and
//! the columns scrolled off cut ([`super::load::shown`]), so [`place`] moves each span there. A
//! span outside its line, inside a character or empty is dropped; a line without an answer yet
//! shows uncoloured. The cache holds the lines of the page and is pruned to them when it grows
//! past [`MAX_CACHED_LINES`].

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use iced::{
    Color, Font, Theme,
    advanced::text::{Highlighter, highlighter::Format},
};
use rc_engine::viewer::TextLine;

use super::document::{Token, TokenClass};
use crate::theme::{self, Syntax};

/// The most lines whose tokens are kept; past it only the page's stay.
pub const MAX_CACHED_LINES: usize = 4096;
/// The most tokens taken of one line; the rest of it shows uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// The stamp of the next set of highlights: a highlighter repaints when the stamp changes.
static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

/// One line's coloured byte ranges.
type LineSpans = Vec<(Range<usize>, TokenClass)>;

/// The spans of the page as the view shows it, per line, for the highlighter.
#[derive(Debug, Clone, Default)]
pub struct Highlights {
    /// 0 for none; unique otherwise, across every viewer.
    stamp: u64,
    lines: Arc<[LineSpans]>,
}

impl PartialEq for Highlights {
    fn eq(&self, other: &Self) -> bool {
        self.stamp == other.stamp
    }
}

impl Highlights {
    /// The spans of line `line` of the page.
    pub fn line(&self, line: usize) -> &[(Range<usize>, TokenClass)] {
        self.lines.get(line).map_or(&[][..], Vec::as_slice)
    }
}

/// The token spans of a document's text: the page on screen, what was asked, what came.
#[derive(Debug, Default)]
pub struct Spans {
    cache: HashMap<u64, Arc<[Token]>>,
    asked: HashSet<u64>,
    page: Vec<TextLine>,
    shown: Highlights,
}

impl Spans {
    /// The page now on screen, as read: each line's start and text.
    pub fn set_page(&mut self, lines: Vec<TextLine>) {
        self.page = lines;
    }

    /// The starts of the page's lines that have neither tokens nor a question yet; they count as
    /// asked from now on.
    pub fn wanted(&mut self) -> Vec<u64> {
        let mut wanted = Vec::new();
        for line in &self.page {
            if !self.cache.contains_key(&line.offset) && self.asked.insert(line.offset) {
                wanted.push(line.offset);
            }
        }
        wanted
    }

    /// The source's answer for the lines at `starts`: one list per line, extra lists ignored, a
    /// line without a list (or a failed answer) kept without tokens, never asked again.
    pub fn arrived(&mut self, starts: &[u64], answer: Result<Vec<Vec<Token>>, &'static str>) {
        let mut lists = answer.unwrap_or_default().into_iter();
        for start in starts {
            if !self.asked.remove(start) {
                continue;
            }
            let mut tokens = lists.next().unwrap_or_default();
            tokens.truncate(MAX_TOKENS_PER_LINE);
            self.cache.insert(*start, tokens.into());
        }
        if self.cache.len() > MAX_CACHED_LINES {
            let page: HashSet<u64> = self.page.iter().map(|line| line.offset).collect();
            self.cache.retain(|start, _| page.contains(start));
        }
    }

    /// Everything forgotten: the text was read again.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The page's spans placed again: on the text shown with `tab_width` and `left` columns cut,
    /// or none while the colours are off.
    pub fn refresh(&mut self, on: bool, tab_width: usize, left: usize) {
        if !on {
            if self.shown.stamp != 0 {
                self.shown = Highlights::default();
            }
            return;
        }
        let lines: Vec<Vec<(Range<usize>, TokenClass)>> = self
            .page
            .iter()
            .map(|line| match self.cache.get(&line.offset) {
                Some(tokens) => place(&line.text, tokens, tab_width, left),
                None => Vec::new(),
            })
            .collect();
        self.shown = Highlights {
            stamp: NEXT_STAMP.fetch_add(1, Ordering::Relaxed),
            lines: lines.into(),
        };
    }

    /// What the highlighter paints.
    pub fn shown(&self) -> &Highlights {
        &self.shown
    }
}

/// `tokens` of the line `raw` placed on the line as the view shows it: tabs expanded to
/// `tab_width`, a control character one dot, the first `left` characters cut. A token outside
/// the line, not on character boundaries or empty is dropped; one cut off at the left keeps what
/// is left of it.
pub fn place(
    raw: &str,
    tokens: &[Token],
    tab_width: usize,
    left: usize,
) -> Vec<(Range<usize>, TokenClass)> {
    // Where each character of `raw` starts on the shown line, and where the line ends.
    let mut starts: Vec<(usize, usize)> = Vec::with_capacity(raw.len() + 1);
    let (mut column, mut shown) = (0usize, 0usize);
    for (at, c) in raw.char_indices() {
        starts.push((at, shown));
        let (columns, bytes) = match c {
            '\t' if tab_width > 0 => (tab_width - column % tab_width, 1),
            c if c.is_control() => (1, 1),
            c => (1, c.len_utf8()),
        };
        for _ in 0..columns {
            if column >= left {
                shown += bytes;
            }
            column += 1;
        }
    }
    starts.push((raw.len(), shown));
    let position = |byte: usize| {
        starts
            .binary_search_by_key(&byte, |(at, _)| *at)
            .ok()
            .map(|index| starts[index].1)
    };
    tokens
        .iter()
        .filter_map(|token| {
            let start = usize::try_from(token.start).ok()?;
            let end = start.checked_add(usize::try_from(token.len).ok()?)?;
            if start >= end || end > raw.len() {
                return None;
            }
            let range = position(start)?..position(end)?;
            (!range.is_empty()).then_some((range, token.class))
        })
        .collect()
}

/// The theme's colour of a token class (`syntax.*`, `docs/design.md`).
pub fn color(syntax: &Syntax, class: TokenClass) -> Color {
    match class {
        TokenClass::Keyword => syntax.keyword,
        TokenClass::Key => syntax.key,
        TokenClass::String => syntax.string,
        TokenClass::Number => syntax.number,
        TokenClass::Literal => syntax.literal,
        TokenClass::Comment => syntax.comment,
        TokenClass::Punctuation => syntax.punctuation,
        TokenClass::Operator => syntax.operator,
        TokenClass::TypeName => syntax.type_name,
        TokenClass::Function => syntax.function,
        TokenClass::Invalid => syntax.invalid,
    }
}

/// A span's format in the look the window is drawn in.
pub fn format(class: &TokenClass, look: &Theme) -> Format<Font> {
    let tokens = theme::tokens(look.extended_palette().is_dark);
    Format {
        color: Some(color(&tokens.syntax, *class)),
        font: None,
    }
}

/// The text view's highlighter: it hands each line of the page its placed spans. It reads the
/// page's [`Highlights`] as its settings, so new spans or a new page repaint from the first line.
#[derive(Debug)]
pub struct SpanHighlighter {
    highlights: Highlights,
    line: usize,
}

impl Highlighter for SpanHighlighter {
    type Settings = Highlights;
    type Highlight = TokenClass;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, TokenClass)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            highlights: settings.clone(),
            line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.highlights = new_settings.clone();
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let spans: Vec<(Range<usize>, TokenClass)> = self
            .highlights
            .line(self.line)
            .iter()
            .filter(|(range, _)| {
                range.end <= line.len()
                    && line.is_char_boundary(range.start)
                    && line.is_char_boundary(range.end)
            })
            .cloned()
            .collect();
        self.line += 1;
        spans.into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

#[cfg(test)]
#[path = "spans_tests.rs"]
mod tests;
