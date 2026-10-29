//! The structure the lexemes describe, one event at a time: a key, a sequence item, a value, a
//! document marker, an error. The structure pass of `open` and the tree's walks both read
//! these, so they cannot disagree about where a node starts.
//!
//! An event's column is its byte column in its line, which is how YAML measures indentation.
//! Anchors and tags before a node belong to it: a key or value starts at its first property.
//! The lines inside a flow collection, a quoted scalar or a block scalar are the value that
//! started them and make no events of their own.

use crate::lexer::{Kind, Lexeme, Lexer, LineState};
use crate::reader::Bytes;
use crate::types::Failure;

/// The characters of a label or value shown; longer ones end in an ellipsis.
pub const PREVIEW_CHARS: usize = 80;

/// What an event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventKind {
    /// A scalar and the `:` after it, or a `?`.
    Key,
    /// A `-`.
    Item,
    /// A value on its line: a scalar, an alias, a flow collection, a block scalar.
    Value,
    DocStart,
    DocEnd,
    Invalid(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Event {
    pub(crate) kind: EventKind,
    pub(crate) start: u64,
    /// The byte column of `start` in its line.
    pub(crate) col: u32,
    /// Where its line starts.
    pub(crate) line_start: u64,
    /// No event came before it on its line.
    pub(crate) first_on_line: bool,
    /// 1-based, counted only from the start of the text.
    pub(crate) line: u64,
    /// 1-based, in characters, counted only from the start of the text.
    pub(crate) column: u64,
    /// A key's label or a value's preview, when the lexer captures.
    pub(crate) text: String,
}

/// Where on its line the next lexeme is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pos {
    /// Before any node: a key, an item or a value may come.
    Start,
    /// After a `-`: as at the start.
    AfterItem,
    /// After a key: a value may come.
    AfterKey,
    /// After a value: the rest of the line is its or a comment.
    Done,
}

/// A label or preview cut at [`PREVIEW_CHARS`].
#[derive(Default)]
struct Preview {
    text: String,
    chars: usize,
    cut: bool,
}

impl Preview {
    fn push(&mut self, text: &str) {
        for character in text.chars() {
            if self.chars == PREVIEW_CHARS {
                self.cut = true;
                return;
            }
            self.text.push(character);
            self.chars += 1;
        }
    }

    fn finish(mut self) -> String {
        if self.cut {
            self.text.truncate(self.text.trim_end().len());
            self.text.push('\u{2026}');
        }
        self.text
    }
}

pub(crate) struct Events<B: Bytes> {
    lexer: Lexer<B>,
    peeked: Option<Lexeme>,
    /// Lexemes from here on are not read: where the valid part of the text ends.
    end: u64,
    done: bool,
    line: u64,
    pos: Pos,
    first: bool,
    /// The first property (anchor, tag) before the next node on this line.
    props: Option<Lexeme>,
    props_text: String,
    /// The depth of a flow collection a value opened and an earlier line left open.
    flow: u32,
    /// Whether labels and previews are made.
    texts: bool,
}

impl<B: Bytes> Events<B> {
    /// The events of `lexer`'s lexemes up to `end`; labels and previews when it captures.
    pub(crate) fn new(lexer: Lexer<B>, end: u64, texts: bool) -> Self {
        Self {
            lexer,
            peeked: None,
            end,
            done: false,
            line: 0,
            pos: Pos::Start,
            first: true,
            props: None,
            props_text: String::new(),
            flow: 0,
            texts,
        }
    }

    pub(crate) fn lexer_mut(&mut self) -> &mut Lexer<B> {
        &mut self.lexer
    }

    /// Goes on at `offset`, the start of a line in plain block context.
    pub(crate) fn jump(&mut self, offset: u64) {
        self.lexer.jump(offset);
        self.peeked = None;
        self.done = offset >= self.end;
        self.pos = Pos::Start;
        self.props = None;
        self.props_text.clear();
        self.flow = 0;
    }

    fn lexeme(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<Option<Lexeme>, Failure> {
        if let Some(lexeme) = self.peeked.take() {
            return Ok(Some(lexeme));
        }
        if self.done {
            return Ok(None);
        }
        match self.lexer.next_with(on_line)? {
            Some(lexeme) if lexeme.start < self.end => Ok(Some(lexeme)),
            _ => {
                self.done = true;
                Ok(None)
            }
        }
    }

    /// The next lexeme when it is on line `line`; it stays to be read again either way.
    fn peek_on(
        &mut self,
        line: u64,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<Option<Lexeme>, Failure> {
        let next = self.lexeme(on_line)?;
        self.peeked = next;
        Ok(next.filter(|lexeme| lexeme.line == line))
    }

    /// The text of the lexeme read last, when texts are made.
    fn text_of(&self, lexeme: &Lexeme) -> String {
        if !self.texts {
            return String::new();
        }
        let captured = self.lexer.captured();
        let length = usize::try_from(lexeme.end - lexeme.start).unwrap_or(usize::MAX);
        String::from_utf8_lossy(
            captured
                .get(..length.min(captured.len()))
                .unwrap_or_default(),
        )
        .into_owned()
    }

    fn event(&mut self, kind: EventKind, lexeme: &Lexeme, text: String) -> Event {
        let first = self.props.take().unwrap_or(*lexeme);
        self.props_text.clear();
        Event {
            kind,
            start: first.start,
            col: first.col,
            line_start: lexeme.start.saturating_sub(u64::from(lexeme.col)),
            first_on_line: std::mem::replace(&mut self.first, false),
            line: first.line,
            column: first.column,
            text,
        }
    }

    pub(crate) fn next(&mut self) -> Result<Option<Event>, Failure> {
        self.next_with(&mut |_, _, _| {})
    }

    /// The next event; `on_line` hears of every line the lexer starts.
    pub(crate) fn next_with(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<Option<Event>, Failure> {
        loop {
            let Some(lexeme) = self.lexeme(on_line)? else {
                return Ok(None);
            };
            if lexeme.line != self.line {
                self.line = lexeme.line;
                self.pos = Pos::Start;
                self.first = true;
                self.props = None;
                self.props_text.clear();
            }
            if let Kind::Invalid(problem) = lexeme.kind {
                self.props = None;
                self.pos = Pos::Done;
                return Ok(Some(self.event(
                    EventKind::Invalid(problem.message()),
                    &lexeme,
                    String::new(),
                )));
            }
            if self.flow > 0 && !matches!(lexeme.kind, Kind::DocStart | Kind::DocEnd) {
                self.count_flow(lexeme.kind);
                continue;
            }
            match lexeme.kind {
                // A document marker ends whatever was open, in the lexer as here.
                Kind::DocStart | Kind::DocEnd => {
                    self.flow = 0;
                    self.props = None;
                    self.pos = Pos::Start;
                    let kind = if lexeme.kind == Kind::DocStart {
                        EventKind::DocStart
                    } else {
                        EventKind::DocEnd
                    };
                    return Ok(Some(self.event(kind, &lexeme, String::new())));
                }
                Kind::Anchor | Kind::Tag => {
                    if self.pos != Pos::Done {
                        if self.props.is_none() {
                            self.props = Some(lexeme);
                        }
                        if self.texts {
                            let text = self.text_of(&lexeme);
                            self.props_text.push_str(&text);
                            self.props_text.push(' ');
                        }
                    }
                }
                Kind::Dash => match self.pos {
                    Pos::Start | Pos::AfterItem => {
                        self.props = None;
                        self.pos = Pos::AfterItem;
                        return Ok(Some(self.event(EventKind::Item, &lexeme, String::new())));
                    }
                    Pos::AfterKey => {
                        self.props = None;
                        self.pos = Pos::Done;
                        return Ok(Some(self.event(
                            EventKind::Invalid("a sequence item is not allowed here"),
                            &lexeme,
                            String::new(),
                        )));
                    }
                    Pos::Done => {}
                },
                Kind::Question => {
                    if matches!(self.pos, Pos::Start | Pos::AfterItem) {
                        self.pos = Pos::AfterKey;
                        return Ok(Some(self.event(EventKind::Key, &lexeme, "?".to_owned())));
                    }
                }
                // A `:` at the start of a line is the value of an explicit key, which the line
                // belongs to; any other one the scalar before it took.
                Kind::Colon => {
                    if self.pos == Pos::Start {
                        self.pos = Pos::Done;
                    }
                }
                Kind::Plain | Kind::Quoted | Kind::Alias | Kind::FlowOpen | Kind::BlockHeader => {
                    if self.pos == Pos::Done {
                        continue;
                    }
                    return self.node(lexeme, on_line).map(Some);
                }
                Kind::Comment
                | Kind::Directive
                | Kind::BlockText
                | Kind::QuotedLine
                | Kind::FlowClose
                | Kind::Comma
                | Kind::Invalid(_) => {}
            }
        }
    }

    fn count_flow(&mut self, kind: Kind) {
        match kind {
            Kind::FlowOpen => self.flow = self.flow.saturating_add(1),
            Kind::FlowClose => self.flow -= 1,
            _ => {}
        }
    }

    /// A node's first lexeme: a key when a `:` follows the scalar on its line, else a value.
    fn node(
        &mut self,
        lexeme: Lexeme,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<Event, Failure> {
        let text = self.text_of(&lexeme);
        let mut preview = Preview::default();
        preview.push(&self.props_text);
        match lexeme.kind {
            Kind::Plain | Kind::Quoted | Kind::Alias => {
                let next = self.peek_on(lexeme.line, on_line)?;
                if let Some(colon) = next.filter(|next| next.kind == Kind::Colon) {
                    self.peeked = None;
                    if self.pos == Pos::AfterKey {
                        self.props = None;
                        self.pos = Pos::Done;
                        return Ok(self.event(
                            EventKind::Invalid("a mapping value is not allowed here"),
                            &colon,
                            String::new(),
                        ));
                    }
                    self.pos = Pos::AfterKey;
                    return Ok(self.event(EventKind::Key, &lexeme, label(&text)));
                }
                preview.push(&text);
                if self
                    .peeked
                    .is_some_and(|next| next.kind == Kind::QuotedLine)
                {
                    preview.cut = true;
                }
            }
            Kind::FlowOpen => {
                preview.push(&text);
                self.flow_preview(lexeme.line, &mut preview, on_line)?;
            }
            _ => {
                // A block header, and the first line of the scalar after it.
                preview.push(&text);
                if let Some(first) = self.peek_on_next(lexeme.line, Kind::BlockText, on_line)? {
                    let text = self.text_of(&first);
                    self.peeked = None;
                    preview.push(" ");
                    preview.push(&text);
                    if self
                        .peek_on_next(first.line, Kind::BlockText, on_line)?
                        .is_some()
                    {
                        preview.cut = true;
                    }
                }
            }
        }
        self.pos = Pos::Done;
        let text = if self.texts {
            preview.finish()
        } else {
            String::new()
        };
        Ok(self.event(EventKind::Value, &lexeme, text))
    }

    /// The next lexeme when it is of `kind` on the line after `line`; it stays to be read
    /// again either way.
    fn peek_on_next(
        &mut self,
        line: u64,
        kind: Kind,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<Option<Lexeme>, Failure> {
        let next = self.lexeme(on_line)?;
        self.peeked = next;
        Ok(next.filter(|lexeme| lexeme.kind == kind && lexeme.line == line + 1))
    }

    /// The rest of a flow collection on its first line, into `preview`; a collection still open
    /// at the line's end makes the lines after it wait for its close.
    fn flow_preview(
        &mut self,
        line: u64,
        preview: &mut Preview,
        on_line: &mut dyn FnMut(u64, u64, LineState),
    ) -> Result<(), Failure> {
        self.flow = 1;
        while self.flow > 0 {
            let Some(next) = self.peek_on(line, on_line)? else {
                preview.cut = true;
                return Ok(());
            };
            self.peeked = None;
            if matches!(next.kind, Kind::Invalid(_)) {
                // The error is an event of its own.
                self.peeked = Some(next);
                self.flow = 0;
                return Ok(());
            }
            self.count_flow(next.kind);
            if self.texts && next.kind != Kind::Comment {
                let text = self.text_of(&next);
                preview.push(&text);
                if matches!(next.kind, Kind::Comma | Kind::Colon) {
                    preview.push(" ");
                }
            }
        }
        Ok(())
    }
}

/// A key's label: a quoted key without its quotes, cut at [`PREVIEW_CHARS`].
fn label(text: &str) -> String {
    let inner = match text.as_bytes().first() {
        Some(&quote @ (b'"' | b'\'')) => {
            let rest = text.get(1..).unwrap_or_default();
            rest.strip_suffix(char::from(quote)).unwrap_or(rest)
        }
        _ => text,
    };
    let mut preview = Preview::default();
    preview.push(inner);
    preview.finish()
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
