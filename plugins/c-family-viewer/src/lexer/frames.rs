//! The open constructs a line starts inside of, as a stack the line index keeps at every
//! checkpoint: block comments with their nesting, strings with their kind, the fields of
//! interpolated strings (Kotlin's `${ }`, C#'s `{ }`) and C#'s format specs.
//!
//! Code brackets outside a field are not kept: no colour depends on them. A field counts the
//! brackets opened inside it, so its closing brace is found.

/// How a string was opened, which decides how it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrKind {
    /// `"..."` with backslash escapes: one line, but in Rust over lines and in C and C++ after a
    /// backslash at the line's end.
    Quoted,
    /// `"""..."""` over lines: Java's text block, Kotlin's raw string, C#'s raw string of `fence`
    /// quotes or more.
    Triple,
    /// C#'s verbatim `@"..."` over lines: no escapes, `""` is a quote.
    Verbatim,
    /// C++'s `R"delim(...)delim"` over lines; the delimiter is the line state's.
    CppRaw,
    /// Rust's `r#"..."#` over lines, `fence` hashes.
    RustRaw,
    /// Go's `` `...` `` over lines.
    Backtick,
}

/// A string open: its kind, its fence and its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Str {
    pub(crate) kind: StrKind,
    /// The hashes of a Rust raw string, the quotes of a triple-quoted one.
    pub(crate) fence: u8,
    /// Fields: 0 for none; in C# the `$` signs, the braces that open a field; in Kotlin 1, for
    /// templates `$name` and `${ }`.
    pub(crate) dollars: u8,
}

impl Str {
    /// A string of `kind` without fence and fields.
    pub(crate) fn plain(kind: StrKind) -> Self {
        Self {
            kind,
            fence: 0,
            dollars: 0,
        }
    }
}

/// A construct that goes on until its closer, possibly over several lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// `/* ... */`, this many nested (Rust and Kotlin nest them, the others close at the first
    /// `*/`).
    Comment(u8),
    /// A string's text.
    Str(Str),
    /// A field's code up to `close` closing braces; `depth` brackets are open inside it.
    Field { depth: u8, close: u8 },
    /// C#'s format spec after a field's `:`: text up to `close` closing braces.
    Spec(u8),
}

/// The frames a stack holds; a string or comment opened past them takes the rest of its line,
/// and a field past them stays text. No source nests strings in fields in strings eight deep.
pub(crate) const SLOTS: usize = 8;

/// The open frames, innermost on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frames {
    slots: [Frame; SLOTS],
    depth: u8,
}

/// What an unused slot holds, so two stacks with the same frames compare equal.
const EMPTY: Frame = Frame::Comment(0);

impl Default for Frames {
    fn default() -> Self {
        Self {
            slots: [EMPTY; SLOTS],
            depth: 0,
        }
    }
}

impl Frames {
    pub(crate) fn depth(self) -> usize {
        usize::from(self.depth)
    }

    /// No slot is left.
    pub(crate) fn full(self) -> bool {
        self.depth() >= SLOTS
    }

    pub(crate) fn top(self) -> Option<Frame> {
        self.depth().checked_sub(1).map(|top| self.slots[top])
    }

    /// Pushes `frame`; false when no slot is left.
    pub(crate) fn push(&mut self, frame: Frame) -> bool {
        if self.full() {
            return false;
        }
        self.slots[self.depth()] = frame;
        self.depth += 1;
        true
    }

    pub(crate) fn pop(&mut self) {
        if let Some(top) = self.depth().checked_sub(1) {
            self.slots[top] = EMPTY;
            self.depth -= 1;
        }
    }

    /// Replaces the top frame.
    pub(crate) fn set_top(&mut self, frame: Frame) {
        if let Some(top) = self.depth().checked_sub(1) {
            self.slots[top] = frame;
        }
    }

    /// The field on top, if the top is one: its open brackets and closing braces.
    pub(crate) fn field(self) -> Option<(u8, u8)> {
        match self.top() {
            Some(Frame::Field { depth, close }) => Some((depth, close)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
