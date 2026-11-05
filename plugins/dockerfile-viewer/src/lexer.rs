//! The Dockerfile lexer, one line at a time, for both passes: the line index of `open` and the
//! tokens of the lines on screen.
//!
//! What a Dockerfile carries from one line into the next is the [`LineState`]: the escape
//! character (`# escape=`), whether parser directives may still come, the instruction a line
//! ending in the escape character continues, a quote it leaves open, and the heredocs whose
//! bodies follow the instruction (`<<EOF`). The line index keeps that state at every
//! checkpoint, so `tokens` starts at any of them without reading what lies before.
//!
//! The lexer colours, it does not parse: an instruction is a keyword, its flags keys, variables
//! keys, strings strings; the shell form of `RUN`, `CMD` and `ENTRYPOINT` and the body of a
//! `RUN <<EOF` heredoc get the shell viewer's colours as far as one line shows them
//! ([`shell`]). It never fails - a Dockerfile that is not well formed is coloured as far as it
//! reads.

mod args;
mod cursor;
mod shell;

use cursor::{Cursor, is_blank};

use crate::types::{Token, TokenClass};

/// The longest heredoc marker remembered; a longer one is no heredoc to the lexer.
pub(crate) const MAX_MARKER: usize = 24;

/// A heredoc whose body follows its instruction: the marker that ends it, whether `<<-` strips
/// leading tabs, whether the body is a shell script (`RUN <<EOF`) or a file's content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Marker {
    pub(crate) bytes: [u8; MAX_MARKER],
    pub(crate) len: u8,
    pub(crate) strip_tabs: bool,
    pub(crate) shell: bool,
}

impl Marker {
    fn name(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or_default()
    }
}

/// How an instruction's arguments are read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Args {
    /// `RUN`, `CMD`, `ENTRYPOINT` and `HEALTHCHECK CMD` in shell form.
    Shell,
    /// The exec form, a JSON array: `CMD ["nginx", "-g", "daemon off;"]`, `SHELL [...]`.
    Json,
    /// `FROM [--platform=...] image [AS name]`.
    From,
    /// `ENV`, `ARG`, `LABEL`: `key=value` pairs (`ENV key value` too).
    Pairs,
    /// Everything else: flags, paths, ports, variables, strings.
    #[default]
    Plain,
}

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    /// The escape character is the backtick (`# escape=``), not the backslash.
    pub(crate) backtick: bool,
    /// A comment, a blank line or an instruction came: no parser directive any more.
    pub(crate) directives_done: bool,
    /// The instruction the previous line continued into this one.
    pub(crate) continued: Option<Args>,
    /// The quote (`"` or `'`) a continued line left open.
    pub(crate) quote: Option<u8>,
    /// In shell form: the next word starts a command.
    pub(crate) command_start: bool,
    /// The heredocs whose bodies come, in order; `pending` of them.
    pub(crate) heredocs: [Marker; 2],
    pub(crate) pending: u8,
}

impl LineState {
    fn escape(&self) -> u8 {
        if self.backtick { b'`' } else { b'\\' }
    }

    /// Queues the body of a heredoc; a third one on an instruction is not followed.
    pub(super) fn add_heredoc(&mut self, marker: Marker) {
        if let Some(slot) = self.heredocs.get_mut(usize::from(self.pending)) {
            *slot = marker;
            self.pending += 1;
        }
    }
}

/// The instructions, upper case, and how each one's arguments are read.
const INSTRUCTIONS: [(&[u8], Args); 18] = [
    (b"FROM", Args::From),
    (b"RUN", Args::Shell),
    (b"CMD", Args::Shell),
    (b"ENTRYPOINT", Args::Shell),
    (b"SHELL", Args::Json),
    (b"ENV", Args::Pairs),
    (b"ARG", Args::Pairs),
    (b"LABEL", Args::Pairs),
    (b"COPY", Args::Plain),
    (b"ADD", Args::Plain),
    (b"WORKDIR", Args::Plain),
    (b"USER", Args::Plain),
    (b"VOLUME", Args::Plain),
    (b"EXPOSE", Args::Plain),
    (b"STOPSIGNAL", Args::Plain),
    (b"MAINTAINER", Args::Plain),
    (b"HEALTHCHECK", Args::Plain),
    (b"ONBUILD", Args::Plain),
];

/// The parser directives a Dockerfile may start with.
const DIRECTIVES: [&[u8]; 3] = [b"syntax", b"escape", b"check"];

/// Lexes one line of the text (without its line break) in `state`, pushing its tokens into
/// `tokens`; the state the next line starts in. `tail` is the last byte of the whole line that is
/// no blank - beyond `text` when the line was longer than the bytes that get tokens.
pub(crate) fn lex_line(
    text: &[u8],
    tail: Option<u8>,
    mut state: LineState,
    tokens: &mut Vec<Token>,
) -> LineState {
    // The carriage return of a CRLF line stays in the text, out of the tokens.
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    let escape = state.escape();
    if state.continued.is_none() && state.pending > 0 {
        heredoc_line(text, &mut state, tokens);
        return state;
    }
    let first = text.iter().position(|byte| !is_blank(*byte));
    let Some(first) = first else {
        // A blank line ends the directives; inside an instruction it changes nothing.
        state.directives_done = true;
        return state;
    };
    if text.get(first) == Some(&b'#') && state.quote.is_none() {
        let mut cursor = Cursor::new(text, text.len(), escape, tokens);
        cursor.at = first;
        if !state.directives_done && state.continued.is_none() && directive(&mut cursor) {
            if let Some(backtick) = escape_directive(text) {
                state.backtick = backtick;
            }
            return state;
        }
        state.directives_done = true;
        cursor.rest();
        cursor.emit(first, TokenClass::Comment);
        // A comment line inside an instruction is dropped; the instruction goes on after it.
        return state;
    }
    state.directives_done = true;
    let continues = tail == Some(escape);
    // The escape character that continues the line is punctuation, outside the arguments.
    let end = if continues {
        text.iter()
            .rposition(|byte| !is_blank(*byte))
            .filter(|at| text.get(*at) == Some(&escape))
            .unwrap_or(text.len())
    } else {
        text.len()
    };
    let mut cursor = Cursor::new(text, end, escape, tokens);
    cursor.at = first;
    let kind = match state.continued {
        Some(kind) => {
            args::arguments(&mut cursor, kind, &mut state);
            kind
        }
        None => {
            state.quote = None;
            instruction(&mut cursor, &mut state)
        }
    };
    if continues && end < text.len() {
        cursor.span(end, end + 1, TokenClass::Punctuation);
    }
    if continues {
        state.continued = Some(kind);
    } else {
        state.continued = None;
        state.quote = None;
    }
    state
}

/// An instruction's word and its arguments; how the arguments are read, for a line continuing
/// them.
fn instruction(cursor: &mut Cursor<'_>, state: &mut LineState) -> Args {
    let start = cursor.at;
    cursor.take_while(|byte| byte.is_ascii_alphabetic());
    let word = cursor.since(start).to_ascii_uppercase();
    let known = INSTRUCTIONS
        .iter()
        .find(|(name, _)| *name == word.as_slice())
        .map(|(_, kind)| *kind);
    let Some(kind) = known else {
        // Not an instruction Docker knows: the word is marked, the rest read plainly.
        cursor.emit(start, TokenClass::Invalid);
        args::arguments(cursor, Args::Plain, state);
        return Args::Plain;
    };
    cursor.emit(start, TokenClass::Keyword);
    cursor.blanks();
    match word.as_slice() {
        // `ONBUILD RUN ...`: the instruction it holds.
        b"ONBUILD" if cursor.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) => {
            instruction(cursor, state)
        }
        b"RUN" | b"CMD" | b"ENTRYPOINT" => args::command(cursor, state),
        b"HEALTHCHECK" => args::healthcheck(cursor, state),
        _ => {
            args::arguments(cursor, kind, state);
            kind
        }
    }
}

/// A parser directive at the cursor's `#`: `# syntax=docker/dockerfile:1`. False, with nothing
/// emitted, for any other comment.
fn directive(cursor: &mut Cursor<'_>) -> bool {
    let hash = cursor.at;
    cursor.bump(1);
    cursor.blanks();
    let key = cursor.at;
    cursor.take_while(|byte| byte.is_ascii_alphabetic());
    let name = cursor.since(key).to_ascii_lowercase();
    let after_key = cursor.at;
    cursor.blanks();
    if !DIRECTIVES.contains(&name.as_slice()) || cursor.peek() != Some(b'=') {
        cursor.at = hash;
        return false;
    }
    let equals = cursor.at;
    cursor.at = hash + 1;
    cursor.emit(hash, TokenClass::Comment);
    cursor.at = after_key;
    cursor.emit(key, TokenClass::Key);
    cursor.at = equals;
    cursor.token(1, TokenClass::Operator);
    cursor.blanks();
    let value = cursor.at;
    cursor.rest();
    cursor.emit(value, TokenClass::String);
    true
}

/// The escape character an `escape` directive line names: `Some(true)` for the backtick,
/// `Some(false)` for the backslash.
fn escape_directive(text: &[u8]) -> Option<bool> {
    let line = String::from_utf8_lossy(text);
    let (key, value) = line.trim_start_matches(['#', ' ', '\t']).split_once('=')?;
    if !key.trim().eq_ignore_ascii_case("escape") {
        return None;
    }
    match value.trim() {
        "`" => Some(true),
        "\\" => Some(false),
        _ => None,
    }
}

/// A line of the first pending heredoc's body, or its closing marker.
fn heredoc_line(text: &[u8], state: &mut LineState, tokens: &mut Vec<Token>) {
    let marker = state.heredocs[0];
    let content = text.strip_suffix(b"\r").unwrap_or(text);
    let skipped = if marker.strip_tabs {
        content.iter().take_while(|byte| **byte == b'\t').count()
    } else {
        0
    };
    let mut cursor = Cursor::new(text, content.len(), state.escape(), tokens);
    cursor.at = skipped;
    if content.get(skipped..) == Some(marker.name()) {
        cursor.rest();
        cursor.emit(skipped, TokenClass::Keyword);
        state.heredocs = [state.heredocs[1], Marker::default()];
        state.pending -= 1;
        state.quote = None;
        state.command_start = false;
        return;
    }
    if marker.shell {
        // Each line of the script starts a command, unless a quote runs on into it.
        if state.quote.is_none() {
            state.command_start = true;
        }
        shell::lex(&mut cursor, state);
    } else {
        cursor.rest();
        cursor.emit(skipped, TokenClass::String);
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
