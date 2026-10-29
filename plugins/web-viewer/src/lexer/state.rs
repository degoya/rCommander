//! What a line starts inside of: the [`LineState`] the line index keeps at every checkpoint, so
//! the lexer starts at any of them as if it had read everything before.
//!
//! The languages nest - CSS and JavaScript in HTML, HTML in PHP, JavaScript in a template
//! string's `${...}` - so the state is a stack of [`Frame`]s, the innermost on top, each with
//! what its language carries from one line into the next. The stack is at most [`MAX_DEPTH`]
//! deep: a frame that would go deeper is not opened, and the rest of its line is `invalid`. PHP
//! is not a frame: it runs over whatever HTML, CSS or JavaScript it is written into, and that
//! stack waits below it until `?>` ([`LineState::php`]).

/// The deepest the frames nest.
pub(crate) const MAX_DEPTH: usize = 64;

/// JavaScript and its relatives: TypeScript adds types, JSX elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    Js,
    Ts,
    Jsx,
    Tsx,
}

impl Dialect {
    pub(crate) fn ts(self) -> bool {
        matches!(self, Self::Ts | Self::Tsx)
    }

    pub(crate) fn jsx(self) -> bool {
        matches!(self, Self::Jsx | Self::Tsx)
    }
}

/// CSS, and the two preprocessors read as CSS with nesting, variables and `//` comments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CssDialect {
    Css,
    Scss,
    Less,
}

/// What ends a frame besides its own closer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// The file's own language: it ends with the file.
    Never,
    /// A `<script>` element's content: `</script`, wherever it stands - in a string or a comment
    /// too, as in a browser.
    ScriptTag,
    /// A `<style>` element's content: `</style`.
    StyleTag,
    /// An attribute value (`style="..."`, `onclick='...'`): its quote.
    Quote(u8),
    /// `${...}` in a template string, `{...}` in JSX: the `}` that matches no `{` of its own.
    Brace,
}

impl End {
    /// Whether a quote or a closing tag ends the frame from within any of its parts.
    pub(crate) fn is_boundary(self) -> bool {
        matches!(self, Self::ScriptTag | Self::StyleTag | Self::Quote(_))
    }
}

/// Where JavaScript code goes on at a line's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsSub {
    Code,
    /// In `/* ... */`.
    Comment,
    /// In a string whose line ended with `\`.
    Str(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JsFrame {
    pub(crate) dialect: Dialect,
    pub(crate) end: End,
    pub(crate) sub: JsSub,
    /// A `/` here starts a regular expression, not a division: what came before it was no value.
    pub(crate) regex_ok: bool,
    /// The `{` open inside a frame that a `}` ends ([`End::Brace`]).
    pub(crate) braces: u8,
}

impl JsFrame {
    pub(crate) fn new(dialect: Dialect, end: End) -> Self {
        Self {
            dialect,
            end,
            sub: JsSub::Code,
            regex_ok: true,
            braces: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CssFrame {
    pub(crate) dialect: CssDialect,
    pub(crate) end: End,
    /// In `/* ... */`.
    pub(crate) comment: bool,
    /// After a declaration's `:`, until its `;` or `}`.
    pub(crate) value: bool,
    /// The blocks open; a `style` attribute counts as one.
    pub(crate) depth: u8,
}

impl CssFrame {
    pub(crate) fn new(dialect: CssDialect, end: End, depth: u8) -> Self {
        Self {
            dialect,
            end,
            comment: false,
            value: false,
            depth,
        }
    }
}

/// The element a start tag opens: what its content is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Elem {
    Other,
    Script(Dialect),
    /// A `<script>` whose `type` is no script (`text/template`): its content is not coloured.
    RawScript,
    Style(CssDialect),
}

/// The attribute whose value comes next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attr {
    Plain,
    /// `style`: CSS declarations.
    Style,
    /// `onclick` and the other `on...`: JavaScript.
    Event,
    /// `lang` (`<script lang="ts">`, `<style lang="scss">` in a `.vue` file).
    Lang,
    /// `type` (`<script type="text/typescript">`).
    Type,
}

/// Where HTML goes on at a line's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlSub {
    Text,
    /// In `<!-- ... -->`.
    Comment,
    /// In `<!DOCTYPE ...>` or another `<!...>`.
    Decl,
    /// Right after `<`: the element's name.
    TagName,
    /// Right after `</`.
    EndTagName,
    /// In a start tag, at its attributes.
    Tag(Elem, Attr),
    /// After an attribute's `=`: its value.
    Eq(Elem, Attr),
    /// In an attribute value opened by this quote.
    Value(Elem, Attr, u8),
    /// In an attribute value whose content is a frame of its own (CSS, JavaScript) above this
    /// one; its closing quote comes once that frame is gone.
    Embedded(Elem, u8),
    /// In an end tag after its name.
    EndTag,
}

/// Where a JSX element goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsxSub {
    /// Right after `<`: the element's name.
    Name,
    Attrs,
    /// After an attribute's `=`.
    Eq,
    /// In an attribute string opened by this quote.
    Str(u8),
}

/// One open construct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    Html(HtmlSub),
    Css(CssFrame),
    Js(JsFrame),
    /// A template string's text, between `` ` `` and `` ` ``.
    Template(Dialect),
    /// A JSX start tag.
    JsxTag(Dialect, JsxSub),
    /// A JSX element's children.
    JsxChildren(Dialect),
    /// A JSX end tag after `</`.
    JsxClose,
    /// Content that is not coloured: a `<script type="text/template">`.
    Raw,
}

impl Frame {
    /// What ends the frame besides its own closer.
    pub(crate) fn end(self) -> End {
        match self {
            Self::Css(css) => css.end,
            Self::Js(js) => js.end,
            Self::Raw => End::ScriptTag,
            _ => End::Never,
        }
    }
}

/// The open frames, innermost on top; never empty in use, never deeper than [`MAX_DEPTH`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stack {
    frames: [Frame; MAX_DEPTH],
    len: u8,
}

impl Stack {
    /// A stack holding `root` alone.
    pub(crate) fn new(root: Frame) -> Self {
        let mut stack = Self {
            frames: [Frame::Raw; MAX_DEPTH],
            len: 0,
        };
        stack.push(root);
        stack
    }

    /// A stack of `frames`, the first at the bottom; those beyond [`MAX_DEPTH`] are dropped.
    pub(crate) fn of(frames: &[Frame]) -> Self {
        let mut stack = Self {
            frames: [Frame::Raw; MAX_DEPTH],
            len: 0,
        };
        frames.iter().for_each(|frame| {
            stack.push(*frame);
        });
        stack
    }

    pub(crate) fn frames(&self) -> &[Frame] {
        self.frames.get(..usize::from(self.len)).unwrap_or_default()
    }

    pub(crate) fn len(&self) -> usize {
        usize::from(self.len)
    }

    pub(crate) fn top(&self) -> Frame {
        self.frames()
            .last()
            .copied()
            .unwrap_or(Frame::Html(HtmlSub::Text))
    }

    /// Replaces the frame on top.
    pub(crate) fn set_top(&mut self, frame: Frame) {
        if let Some(index) = usize::from(self.len).checked_sub(1)
            && let Some(slot) = self.frames.get_mut(index)
        {
            *slot = frame;
        }
    }

    /// Opens `frame` on top; false, and nothing opened, when the stack is full.
    pub(crate) fn push(&mut self, frame: Frame) -> bool {
        let Some(slot) = self.frames.get_mut(usize::from(self.len)) else {
            return false;
        };
        *slot = frame;
        self.len += 1;
        true
    }

    /// Closes the frame on top; the bottom one stays.
    pub(crate) fn pop(&mut self) {
        if self.len > 1 {
            self.len -= 1;
        }
    }

    /// Closes the frames from `index` up; the bottom one stays.
    pub(crate) fn truncate(&mut self, index: usize) {
        let keep = u8::try_from(index.max(1)).unwrap_or(self.len);
        self.len = self.len.min(keep);
    }

    /// The topmost frame a quote or a closing tag ends, and its index.
    pub(crate) fn boundary(&self) -> Option<(usize, End)> {
        self.frames()
            .iter()
            .enumerate()
            .rev()
            .map(|(index, frame)| (index, frame.end()))
            .find(|(_, end)| end.is_boundary())
    }
}

/// Two stacks are equal when their open frames are; what a closed frame left behind is not
/// compared.
impl PartialEq for Stack {
    fn eq(&self, other: &Self) -> bool {
        self.frames() == other.frames()
    }
}

impl Eq for Stack {}

/// A PHP heredoc's or nowdoc's closing marker, by length and hash, so the state stays `Copy` and
/// small whatever the marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
    pub(crate) hash: u64,
    pub(crate) len: u16,
    /// A nowdoc (`<<<'EOT'`): no variable in its text.
    pub(crate) nowdoc: bool,
}

/// Where PHP goes on at a line's start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PhpSub {
    #[default]
    Code,
    /// In `/* ... */`.
    Comment,
    /// In a string opened by this quote: `'`, `"` or `` ` ``.
    Str(u8),
    /// In a heredoc or nowdoc, until its marker.
    Heredoc(Marker),
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) stack: Stack,
    /// Inside `<?php ... ?>`: the frames below wait.
    pub(crate) php: Option<PhpSub>,
}

impl LineState {
    pub(crate) fn new(root: Frame) -> Self {
        Self {
            stack: Stack::new(root),
            php: None,
        }
    }
}

/// FNV-1a over a heredoc marker's bytes.
pub(crate) fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
