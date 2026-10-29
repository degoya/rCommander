//! The log lexer: one line at a time, left to right, every step forward. A log is a list of
//! records, a line each, so a line is coloured from its own bytes: the level word, timestamps,
//! addresses, URLs and paths, `key=value` and `"key":`, quoted strings, square brackets, and
//! stack-trace lines as a whole.
//!
//! Only one thing reaches from one line into the next ([`LineState`]), and only for
//! [`LevelColour::Line`]: the record of an error or a warning, whose stack trace and exception
//! lines take the level's colour after it.
//!
//! Every matcher looks ahead a bounded distance or to a delimiter it then consumes, and a word no
//! matcher takes is skipped whole, so a line costs its length - nothing scans back. The lexer
//! decides on ASCII bytes alone; any other byte is part of a word and never starts or ends a token
//! in the middle of a character. So it reaches the same state on the file's bytes (`open`'s index)
//! as on their UTF-8 text (`tokens`), and every token sits on character boundaries.

mod addresses;
mod stamps;
mod trace;

use crate::types::{LevelColour, Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// A record's severity, as its level word names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn class(self) -> TokenClass {
        match self {
            Self::Error => TokenClass::Invalid,
            Self::Warn => TokenClass::Function,
            Self::Info => TokenClass::TypeName,
            Self::Debug => TokenClass::Comment,
        }
    }

    /// Whether `line` colours the whole line of a record at this level.
    fn colours_line(self) -> bool {
        matches!(self, Self::Error | Self::Warn)
    }
}

/// The level words, matched whole and in upper, lower or capitalised case: the job's list and
/// syslog's and nginx's own short forms of an error.
const LEVELS: [(&[u8], Level); 16] = [
    (b"ERROR", Level::Error),
    (b"FATAL", Level::Error),
    (b"CRITICAL", Level::Error),
    (b"SEVERE", Level::Error),
    (b"PANIC", Level::Error),
    (b"ERR", Level::Error),
    (b"CRIT", Level::Error),
    (b"ALERT", Level::Error),
    (b"EMERG", Level::Error),
    (b"WARN", Level::Warn),
    (b"WARNING", Level::Warn),
    (b"INFO", Level::Info),
    (b"NOTICE", Level::Info),
    (b"DEBUG", Level::Debug),
    (b"TRACE", Level::Debug),
    (b"VERBOSE", Level::Debug),
];

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    /// The error or warning whose stack trace this line may go on with.
    pub(crate) record: Option<Level>,
}

/// Lexes `line` in `state` and returns the state the next line starts in. `tokens`, when given,
/// gets the line's tokens; without it only the state is worked out - the index's pass.
pub(crate) fn lex_line(
    line: &[u8],
    state: LineState,
    colour: LevelColour,
    tokens: Option<&mut Vec<Token>>,
) -> LineState {
    let (next, whole) = match colour {
        LevelColour::Line => follow(line, state),
        LevelColour::Word => (LineState::default(), None),
    };
    if let Some(tokens) = tokens {
        match whole {
            Some(level) if !line.is_empty() => tokens.push(Token {
                start: 0,
                len: offset(line.len()),
                class: level.class(),
            }),
            _ => {
                scan(line, Some(tokens));
            }
        }
    }
    next
}

/// The state after `line`, and the level whose colour the whole line takes.
fn follow(line: &[u8], state: LineState) -> (LineState, Option<Level>) {
    if let Some(level) = scan(line, None).filter(|level| level.colours_line()) {
        return (
            LineState {
                record: Some(level),
            },
            Some(level),
        );
    }
    if trace::starts_trace(line) {
        let record = Some(Level::Error);
        return (LineState { record }, record);
    }
    if let Some(record) = state.record
        && trace::continues(line)
    {
        return (state, Some(record));
    }
    (LineState::default(), None)
}

/// Colours `line` into `tokens`, or with `None` only finds its level; the first level the line
/// names either way.
fn scan(line: &[u8], tokens: Option<&mut Vec<Token>>) -> Option<Level> {
    let mut scan = Scan {
        line,
        tokens,
        level: None,
        logcat: false,
        unclosed: [false; 2],
    };
    scan.run();
    scan.level
}

struct Scan<'a> {
    line: &'a [u8],
    /// `None` once the line has its 4096 tokens, or when only the level is wanted.
    tokens: Option<&'a mut Vec<Token>>,
    level: Option<Level>,
    /// The line starts with Android logcat's `MM-DD HH:MM:SS.mmm`: a lone letter is its level.
    logcat: bool,
    /// No closing `"` (0) or `'` (1) follows any more: a later quote is not looked through again.
    unclosed: [bool; 2],
}

impl Scan<'_> {
    fn run(&mut self) {
        let line = self.line;
        if trace::is_frame(line) {
            let start = line.len() - line.trim_ascii_start().len();
            self.emit(start, line.len(), TokenClass::TypeName);
            return;
        }
        let mut at = self.line_start();
        // Without tokens to write the scan ends at the first level: it is all `follow` asks.
        while at < line.len() && (self.tokens.is_some() || self.level.is_none()) {
            at = self.step(at);
        }
    }

    fn emit(&mut self, start: usize, end: usize, class: TokenClass) {
        let Some(tokens) = self.tokens.as_deref_mut() else {
            return;
        };
        if end > start {
            tokens.push(Token {
                start: offset(start),
                len: offset(end - start),
                class,
            });
        }
        if tokens.len() >= MAX_TOKENS_PER_LINE {
            self.tokens = None;
        }
    }

    fn found(&mut self, level: Level, start: usize, end: usize) {
        if self.level.is_none() {
            self.level = Some(level);
        }
        self.emit(start, end, level.class());
    }

    /// The forms only a line's start has: glog's `E1020 15:20:01.123456`, logcat's `E/Tag:` and
    /// `10-20 15:20:01.123`, Unix seconds. Where the scan goes on.
    fn line_start(&mut self) -> usize {
        let line = self.line;
        let letter = line.first().copied().and_then(letter_level);
        if let (Some(level), Some(end)) = (letter, stamps::glog(line)) {
            self.found(level, 0, 1);
            self.emit(1, end, TokenClass::Number);
            return end;
        }
        if let Some(level) = letter.filter(|_| logcat_tag(line)) {
            self.found(level, 0, 1);
            return 1;
        }
        if let Some(end) = stamps::logcat(line) {
            self.logcat = true;
            self.emit(0, end, TokenClass::Number);
            return end;
        }
        let bracket = usize::from(line.first() == Some(&b'['));
        if let Some(end) = stamps::unix_seconds(line, bracket) {
            self.emit(0, bracket, TokenClass::Punctuation);
            self.emit(bracket, end, TokenClass::Number);
            return end;
        }
        0
    }

    /// Lexes what starts at `at`; returns where the next step starts, always after `at`.
    fn step(&mut self, at: usize) -> usize {
        let line = self.line;
        let Some(&byte) = line.get(at) else {
            return line.len();
        };
        let literal = |end: Option<usize>| end.map(|end| (end, TokenClass::Literal));
        let matched = match byte {
            b'[' | b']' => Some((at + 1, TokenClass::Punctuation)),
            b'"' | b'\'' => return self.quoted(at),
            b'0'..=b'9' => stamps::at_digit(line, at)
                .map(|end| (end, TokenClass::Number))
                .or_else(|| literal(addresses::ipv4(line, at)))
                .or_else(|| literal(addresses::ipv6(line, at))),
            b'/' | b'.' | b'~' | b'\\' => literal(addresses::path(line, at)),
            b':' => literal(addresses::ipv6(line, at)),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => return self.word(at),
            _ => None,
        };
        match matched {
            Some((end, class)) => {
                self.emit(at, end, class);
                end
            }
            None if is_word(byte) => word_end(line, at),
            None => at + 1,
        }
    }

    /// A word: a URL, a Windows path, a syslog date, an IPv6 address, a key, a level - or none of
    /// them, skipped whole.
    fn word(&mut self, at: usize) -> usize {
        let line = self.line;
        let end = word_end(line, at);
        if let Some(stop) = addresses::url(line, at)
            .or_else(|| addresses::path(line, at))
            .or_else(|| addresses::ipv6(line, at))
        {
            self.emit(at, stop, TokenClass::Literal);
            return stop;
        }
        if let Some(stop) = stamps::syslog(line, at) {
            self.emit(at, stop, TokenClass::Number);
            return stop;
        }
        if let Some(stop) = key_end(line, at) {
            self.emit(at, stop, TokenClass::Key);
            return stop;
        }
        let word = line.get(at..end).unwrap_or_default();
        let level = if self.logcat && self.level.is_none() && word.len() == 1 {
            word.first().copied().and_then(letter_level)
        } else {
            level_of(word).filter(|_| stands_alone(line, at, end))
        };
        if let Some(level) = level {
            self.found(level, at, end);
        }
        end
    }

    /// A quoted string: a key when `:` follows (`"key":`), a level or a timestamp when it holds
    /// nothing else, else a string. A quote without its closing quote is skipped.
    fn quoted(&mut self, at: usize) -> usize {
        let line = self.line;
        let single = line.get(at) == Some(&b'\'');
        // An apostrophe inside a word (`can't`) opens nothing.
        if single && at > 0 && line.get(at - 1).copied().is_some_and(is_word) {
            return at + 1;
        }
        let slot = usize::from(single);
        if self.unclosed[slot] {
            return at + 1;
        }
        let Some(close) = closing(line, at + 1, single) else {
            self.unclosed[slot] = true;
            return at + 1;
        };
        let end = close + 1;
        let inside = line.get(at + 1..close).unwrap_or_default();
        if !single
            && line
                .get(end..)
                .is_some_and(|rest| rest.trim_ascii_start().first() == Some(&b':'))
        {
            self.emit(at, end, TokenClass::Key);
        } else if let Some(level) = level_of(inside) {
            self.found(level, at, end);
        } else if stamps::any(line, at + 1) == Some(close) {
            self.emit(at, end, TokenClass::Number);
        } else {
            self.emit(at, end, TokenClass::String);
        }
        end
    }
}

/// The index of the quote closing a string whose text starts at `from`: the next `"` not escaped
/// with `\`; for `'` the next one no word goes on after.
fn closing(line: &[u8], from: usize, single: bool) -> Option<usize> {
    let mut at = from;
    while let Some(&byte) = line.get(at) {
        match byte {
            b'\\' => at += 1,
            b'"' if !single => return Some(at),
            b'\'' if single && !line.get(at + 1).copied().is_some_and(is_word) => return Some(at),
            _ => {}
        }
        at += 1;
    }
    None
}

/// The end of a key starting at `at` when `=` follows it (not `==`): letters, digits, `_`, `.`
/// and `-`, starting with a letter or `_`.
fn key_end(line: &[u8], at: usize) -> Option<usize> {
    if at > 0
        && line
            .get(at - 1)
            .is_some_and(|byte| matches!(byte, b'.' | b'-'))
    {
        return None;
    }
    let length = line
        .get(at..)?
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        .count();
    let end = at + length;
    (line.get(end) == Some(&b'=') && line.get(end + 1) != Some(&b'=')).then_some(end)
}

/// The level `word` names: one of [`LEVELS`] in upper, lower or capitalised case.
fn level_of(word: &[u8]) -> Option<Level> {
    let (_, rest) = word.split_first()?;
    let upper = !word.iter().any(u8::is_ascii_lowercase);
    let lower_or_capital = !rest.iter().any(u8::is_ascii_uppercase);
    if !upper && !lower_or_capital {
        return None;
    }
    LEVELS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(word))
        .map(|(_, level)| *level)
}

/// Whether the word `at..end` is a word of its own, not a piece of a name: no `.`, `-`, `/` or
/// `\` joins it to the word before or after (`error.log`, `on-error`).
fn stands_alone(line: &[u8], at: usize, end: usize) -> bool {
    let joins = |byte: &u8| matches!(byte, b'.' | b'-' | b'/' | b'\\');
    let before = at > 0 && line.get(at - 1).is_some_and(joins);
    let after = line.get(end).is_some_and(joins) && line.get(end + 1).copied().is_some_and(is_word);
    !before && !after
}

/// The level of a single letter: logcat's and glog's `V D I W E F A`.
fn letter_level(letter: u8) -> Option<Level> {
    match letter {
        b'V' | b'D' => Some(Level::Debug),
        b'I' => Some(Level::Info),
        b'W' => Some(Level::Warn),
        b'E' | b'F' | b'A' => Some(Level::Error),
        _ => None,
    }
}

/// Whether the line is logcat's brief form, `E/Tag: ...` or `E/Tag( 123): ...`: a tag without
/// spaces after the letter's `/`, ended by `:` or `(` (so `I/O error` is not a level).
fn logcat_tag(line: &[u8]) -> bool {
    if line.get(1) != Some(&b'/') {
        return false;
    }
    let rest = line.get(2..).unwrap_or_default();
    let tag = rest
        .iter()
        .take_while(|byte| !matches!(byte, b':' | b'(') && !byte.is_ascii_whitespace())
        .count();
    tag > 0 && matches!(rest.get(tag), Some(b':' | b'('))
}

/// Letters, digits, `_` and any byte of a character beyond ASCII.
pub(crate) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// The end of the word starting at `at`; at least one byte on.
fn word_end(line: &[u8], at: usize) -> usize {
    let length = line
        .get(at..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| is_word(**byte))
        .count();
    at + length.max(1)
}

/// A byte offset within a lexed line as a token's offset: a line is lexed up to 64 KiB of the
/// file, a few times that as UTF-8 at most.
fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
