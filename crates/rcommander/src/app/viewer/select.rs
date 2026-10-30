//! Selecting text in the Markdown preview (RC-020-14): dragging over paragraphs, headings, list
//! items, table cells and code lines, Shift+click to extend, a double click for a word, a triple
//! click for a block; Ctrl+A selects all and Ctrl+C copies the selection as plain text, the way
//! a browser copies a rendered page. `viewer.markdown_select` turns it off.
//!
//! The preview is a column of blocks, each one line of text without a line break: a paragraph
//! split at its hard breaks, a heading, a code line. A spot is a block and a byte offset in its
//! text. The blocks are numbered in the order iced's `markdown` view builds them - list items
//! and table cells (the header, then row by row) included - by [`blocks`], which
//! runs that same view once when the file is parsed; the drawing numbers them again
//! (`markdown_view.rs`), so a spot means the same text in both.

use std::{cell::RefCell, ops::Range};

use iced::{
    Element,
    advanced::mouse::click::Kind,
    widget::{
        Column,
        markdown::{self, Highlight, Item, Settings, Style, Text, Uri, Viewer},
    },
};

/// A span of the preview's text.
pub type Span = iced::advanced::text::Span<'static, Uri>;

/// A place in the preview: block `block`, byte `at` of its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spot {
    pub block: usize,
    pub at: usize,
}

/// What the mouse did on a block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// The left button went down; `extend` with Shift held.
    Press {
        spot: Spot,
        extend: bool,
        kind: Kind,
    },
    /// The cursor moved over a block while the button is down.
    Drag(Spot),
    /// The button came up.
    Release,
}

/// The preview's selection: from `anchor` to `head`, in either order.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    anchor: Option<Spot>,
    head: Option<Spot>,
    /// The button is down after a single click: moving extends the selection.
    pub dragging: bool,
}

impl Selection {
    /// `gesture` applied, against the preview's `blocks`.
    pub fn apply(&mut self, gesture: Gesture, blocks: &[String]) {
        match gesture {
            Gesture::Press {
                spot, extend: true, ..
            } if self.anchor.is_some() => {
                self.head = Some(spot);
                self.dragging = true;
            }
            Gesture::Press { spot, kind, .. } => {
                let text = blocks.get(spot.block).map_or("", String::as_str);
                let range = match kind {
                    Kind::Single => spot.at..spot.at,
                    Kind::Double => word_at(text, spot.at),
                    Kind::Triple => 0..text.len(),
                };
                self.anchor = Some(Spot {
                    block: spot.block,
                    at: range.start,
                });
                self.head = Some(Spot {
                    block: spot.block,
                    at: range.end,
                });
                self.dragging = kind == Kind::Single;
            }
            Gesture::Drag(spot) if self.dragging => self.head = Some(spot),
            Gesture::Drag(_) => {}
            Gesture::Release => self.dragging = false,
        }
    }

    /// Ctrl+A: every block.
    pub fn all(&mut self, blocks: &[String]) {
        let Some(last) = blocks.len().checked_sub(1) else {
            return;
        };
        self.anchor = Some(Spot { block: 0, at: 0 });
        self.head = Some(Spot {
            block: last,
            at: blocks[last].len(),
        });
        self.dragging = false;
    }

    /// The block the selection started in: its block reports the release.
    pub fn anchor_block(&self) -> Option<usize> {
        self.anchor.map(|spot| spot.block)
    }

    /// Start and end in reading order; `None` while nothing is selected.
    fn ordered(&self) -> Option<(Spot, Spot)> {
        let (anchor, head) = (self.anchor?, self.head?);
        (anchor != head).then(|| (anchor.min(head), anchor.max(head)))
    }

    /// The bytes selected of block `block`, whose text is `len` bytes long.
    pub fn range_in(&self, block: usize, len: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered()?;
        if block < start.block || block > end.block {
            return None;
        }
        let from = if block == start.block { start.at } else { 0 };
        let to = if block == end.block { end.at } else { len };
        let range = from.min(len)..to.min(len);
        (!range.is_empty()).then_some(range)
    }

    /// The selected text, one line a block; `None` while nothing is selected.
    pub fn text(&self, blocks: &[String]) -> Option<String> {
        let (start, end) = self.ordered()?;
        let lines: Vec<&str> = (start.block..=end.block.min(blocks.len().saturating_sub(1)))
            .filter_map(|block| {
                let text = blocks.get(block)?;
                let range = self.range_in(block, text.len()).unwrap_or(0..0);
                text.get(range)
            })
            .collect();
        Some(lines.join("\n"))
    }
}

/// The word around byte `at` of `text`: letters, digits and `_`; the one character there when
/// it is none of them.
pub fn word_at(text: &str, at: usize) -> Range<usize> {
    let at = text.floor_char_boundary(at.min(text.len()));
    let wordy = |c: char| c.is_alphanumeric() || c == '_';
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| wordy(*c))
        .last()
        .map_or(at, |(i, _)| i);
    let end = text[at..]
        .char_indices()
        .find(|(_, c)| !wordy(*c))
        .map_or(text.len(), |(i, _)| at + i);
    if start == end {
        let next = text[at..].chars().next().map_or(0, char::len_utf8);
        return at..at + next;
    }
    start..end
}

/// The spans of `text` split at its hard breaks: one run of spans a block.
pub fn lines(spans: &[Span]) -> Vec<Vec<Span>> {
    let mut lines = vec![Vec::new()];
    for span in spans {
        let mut parts = span.text.split('\n');
        if let Some(first) = parts.next()
            && let Some(line) = lines.last_mut()
            && !first.is_empty()
        {
            line.push(with_text(span, first));
        }
        for part in parts {
            lines.push(if part.is_empty() {
                Vec::new()
            } else {
                vec![with_text(span, part)]
            });
        }
    }
    lines
}

/// `spans` with the bytes `range` of their joined text marked by `highlight`; a span cut by the
/// range keeps its padding only on its outer sides, so the text does not move.
pub fn marked(spans: &[Span], range: Range<usize>, highlight: Highlight) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len() + 2);
    let mut from = 0;
    for span in spans {
        let len = span.text.len();
        let (start, end) = (
            range.start.clamp(from, from + len) - from,
            range.end.clamp(from, from + len) - from,
        );
        from += len;
        if start >= end {
            out.push(span.clone());
            continue;
        }
        let cuts = [(0, start, false), (start, end, true), (end, len, false)];
        let pieces: Vec<_> = cuts.iter().filter(|(a, b, _)| a < b).collect();
        let count = pieces.len();
        for (index, (a, b, selected)) in pieces.into_iter().enumerate() {
            let mut piece = with_text(span, span.text.get(*a..*b).unwrap_or_default());
            if index > 0 {
                piece.padding.left = 0.0;
            }
            if index + 1 < count {
                piece.padding.right = 0.0;
            }
            if *selected {
                piece.highlight = Some(highlight);
            }
            out.push(piece);
        }
    }
    out
}

fn with_text(span: &Span, text: &str) -> Span {
    Span {
        text: text.to_owned().into(),
        ..span.clone()
    }
}

/// The plain text of each block of `items`, numbered as the preview draws them.
pub fn blocks(items: &[Item]) -> Vec<String> {
    let collector = Collector::default();
    let _: Element<'_, ()> =
        markdown::view_with(items, Settings::from(&iced::Theme::Light), &collector);
    collector.blocks.into_inner()
}

/// The texts of `text`'s blocks, in `style`.
pub fn text_blocks(text: &Text, style: Style) -> Vec<String> {
    lines(&text.spans(style))
        .iter()
        .map(|line| line.iter().map(|span| span.text.as_ref()).collect())
        .collect()
}

/// A viewer that draws nothing and writes down each block's text.
#[derive(Default)]
struct Collector {
    blocks: RefCell<Vec<String>>,
}

impl Collector {
    fn note(&self, text: &Text, style: Style) -> Element<'static, ()> {
        self.blocks.borrow_mut().extend(text_blocks(text, style));
        Column::new().into()
    }
}

impl<'a> Viewer<'a, ()> for Collector {
    fn on_link_click(_url: Uri) {}

    fn image(&self, _: Settings, _: &'a Uri, _: &'a str, _: &Text) -> Element<'a, ()> {
        Column::new().into()
    }

    fn heading(
        &self,
        settings: Settings,
        _level: &'a markdown::HeadingLevel,
        text: &'a Text,
        _index: usize,
    ) -> Element<'a, ()> {
        self.note(text, settings.style)
    }

    fn paragraph(&self, settings: Settings, text: &Text) -> Element<'a, ()> {
        self.note(text, settings.style)
    }

    fn code_block(
        &self,
        settings: Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [Text],
    ) -> Element<'a, ()> {
        for line in lines {
            self.blocks
                .borrow_mut()
                .extend(text_blocks(line, settings.style));
        }
        Column::new().into()
    }
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
