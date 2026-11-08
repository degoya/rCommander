//! Colours for the fenced code blocks of the Markdown preview (RC-110-06): one small lexer driven
//! by a table of languages (`code_lexer/langs.rs`) - line and block comments, strings, numbers,
//! keyword, literal and type lists, keys of configuration files, markup tags, shell variables and
//! diff lines. It marks token classes only; the theme gives each its colour (`syntax.*`), as for a
//! plugin's coloured text (RC-100-10). A block whose tag names no language here, or that has no
//! tag, stays plain: nothing is guessed.
//!
//! The file is foreign input, so the work is bounded: the blocks are coloured once, with the
//! parse and off the UI thread ([`colours`]); a block over [`MAX_BLOCK_BYTES`] and a line over
//! [`MAX_LINE_BYTES`] stay plain. `viewer.markdown_code_highlight` turns it off.

mod langs;

use std::{cell::RefCell, ops::Range, sync::Arc};

use iced::{
    Element,
    widget::{
        Column,
        markdown::{self, Item, Settings, Text, Uri, Viewer},
    },
};

pub use self::langs::lang_of;
use super::document::TokenClass;

/// The tokens of one line: byte ranges of its text and their classes, in order.
pub type LineTokens = Vec<(Range<usize>, TokenClass)>;
/// The tokens of one code block, a list a line; `None` for a block that stays plain.
pub type BlockTokens = Option<Arc<[LineTokens]>>;

/// The largest code block coloured; a larger one stays plain.
pub const MAX_BLOCK_BYTES: usize = 64 * 1024;
/// The longest line coloured; a longer one stays plain.
pub const MAX_LINE_BYTES: usize = 4 * 1024;

/// How one language is read.
#[derive(Debug)]
pub struct Lang {
    pub line_comments: &'static [&'static str],
    /// Comments and strings that may span lines: opening, closing, class.
    pub blocks: &'static [(&'static str, &'static str, TokenClass)],
    /// The characters that open and close a string on one line.
    pub quotes: &'static [char],
    /// The character that escapes the next one inside a string.
    pub escape: Option<char>,
    pub keywords: &'static [&'static str],
    pub literals: &'static [&'static str],
    pub types: &'static [&'static str],
    /// Keywords, literals and types match without regard to case (SQL, PowerShell).
    pub fold_case: bool,
    /// A name or string just before one of these is a key (`key: value`, `key = value`).
    pub key_marks: &'static [char],
    /// `[section]` at the start of a line is a type name (TOML, INI).
    pub sections: bool,
    /// The name after `<` or `</` is a keyword (XML, HTML).
    pub tags: bool,
    /// The character that starts a variable (`$name`, `${name}`).
    pub variables: Option<char>,
    /// A name before `(` is a function.
    pub calls: bool,
    /// A name that starts with a capital is a type.
    pub upper_types: bool,
    /// `'a` is a lifetime unless it is a character (`'x'`, `'\n'`).
    pub lifetimes: bool,
    /// Names may hold `-` (keys, tags, cmdlets).
    pub dashes: bool,
    /// Every line by its first characters (a unified diff).
    pub diff: bool,
}

/// The tokens of `lines`, one block's in order; `None` when the block is over the limit.
pub fn lex_block(lang: &Lang, lines: &[String]) -> Option<Vec<LineTokens>> {
    let total: usize = lines.iter().map(|line| line.len() + 1).sum();
    if total > MAX_BLOCK_BYTES {
        return None;
    }
    let mut open = None;
    let tokens = lines
        .iter()
        .map(|line| {
            if line.len() > MAX_LINE_BYTES {
                open = None;
                return Vec::new();
            }
            lex_line(lang, line, &mut open)
        })
        .collect();
    Some(tokens)
}

/// The tokens of `line`; `open` is the block (by its index in `lang.blocks`) the line before
/// left open, and what this one leaves open.
pub fn lex_line(lang: &Lang, line: &str, open: &mut Option<usize>) -> LineTokens {
    if lang.diff {
        return diff_line(line);
    }
    let mut out = LineTokens::new();
    let mut at = 0;
    if let Some(index) = open.take()
        && let Some((_, close, class)) = lang.blocks.get(index)
    {
        match line.find(close) {
            Some(end) => {
                at = end + close.len();
                push(&mut out, 0..at, *class);
            }
            None => {
                push(&mut out, 0..line.len(), *class);
                *open = Some(index);
                return out;
            }
        }
    }
    let mut after_tag = false;
    while let Some(c) = line[at..].chars().next() {
        let rest = &line[at..];
        if c.is_whitespace() {
            at += c.len_utf8();
            continue;
        }
        let tag = std::mem::take(&mut after_tag);
        if is_line_comment(lang, line, at) {
            push(&mut out, at..line.len(), TokenClass::Comment);
            break;
        }
        if let Some((index, (start, close, class))) = lang
            .blocks
            .iter()
            .enumerate()
            .find(|(_, (start, _, _))| rest.starts_with(start))
        {
            let from = at + start.len();
            match line[from..].find(close) {
                Some(end) => {
                    let to = from + end + close.len();
                    push(&mut out, at..to, *class);
                    at = to;
                }
                None => {
                    push(&mut out, at..line.len(), *class);
                    *open = Some(index);
                    break;
                }
            }
            continue;
        }
        let (len, class) = if lang.sections && c == '[' && line[..at].trim().is_empty() {
            (
                rest.find(']').map_or(rest.len(), |end| end + 1),
                Some(TokenClass::TypeName),
            )
        } else if c == '\'' && lang.lifetimes && !is_char_literal(rest) {
            (1 + name_len(&rest[1..], false), Some(TokenClass::Literal))
        } else if lang.quotes.contains(&c) {
            let len = string_len(rest, c, lang.escape);
            let key = is_key(lang, &line[at + len..], true);
            (
                len,
                Some(if key {
                    TokenClass::Key
                } else {
                    TokenClass::String
                }),
            )
        } else if Some(c) == lang.variables {
            (variable_len(rest), Some(TokenClass::Literal))
        } else if c.is_ascii_digit() {
            (number_len(rest), Some(TokenClass::Number))
        } else if c.is_alphabetic() || c == '_' {
            let len = name_len(rest, lang.dashes);
            (len, name_class(lang, &rest[..len], &line[at + len..], tag))
        } else {
            after_tag = lang.tags && (c == '<' || (c == '/' && tag));
            (c.len_utf8(), symbol_class(c))
        };
        if let Some(class) = class {
            push(&mut out, at..at + len, class);
        }
        at += len.max(c.len_utf8());
    }
    out
}

/// The class of the name `word`, followed by `after` on its line; `tag` when it follows `<`.
fn name_class(lang: &Lang, word: &str, after: &str, tag: bool) -> Option<TokenClass> {
    let listed = |list: &[&str]| {
        list.iter().any(|entry| {
            if lang.fold_case {
                entry.eq_ignore_ascii_case(word)
            } else {
                *entry == word
            }
        })
    };
    if tag {
        Some(TokenClass::Keyword)
    } else if is_key(lang, after, false) {
        Some(TokenClass::Key)
    } else if listed(lang.keywords) {
        Some(TokenClass::Keyword)
    } else if listed(lang.literals) {
        Some(TokenClass::Literal)
    } else if listed(lang.types) {
        Some(TokenClass::TypeName)
    } else if lang.calls && after.starts_with('(') {
        Some(TokenClass::Function)
    } else if lang.upper_types && word.starts_with(|c: char| c.is_uppercase()) {
        Some(TokenClass::TypeName)
    } else {
        None
    }
}

/// Whether what follows a name or a string makes it a key: one of the key marks, for a bare
/// name a `:` only with space or the end after it (`url: http://x` has one key).
fn is_key(lang: &Lang, after: &str, quoted: bool) -> bool {
    let after = after.trim_start();
    let Some(mark) = after.chars().next() else {
        return false;
    };
    if !lang.key_marks.contains(&mark) {
        return false;
    }
    quoted || mark != ':' || after[1..].chars().next().is_none_or(char::is_whitespace)
}

/// A line comment starts at `at`; `#` only at the start of the line or after a space, so
/// `$#` and `a#b` stay what they are.
fn is_line_comment(lang: &Lang, line: &str, at: usize) -> bool {
    lang.line_comments.iter().any(|marker| {
        line[at..].starts_with(marker)
            && (!marker.starts_with('#') || at == 0 || line[..at].ends_with(char::is_whitespace))
    })
}

fn symbol_class(c: char) -> Option<TokenClass> {
    if "{}[]();,.".contains(c) {
        Some(TokenClass::Punctuation)
    } else if "+-*/%=<>!&|^~?:@".contains(c) {
        Some(TokenClass::Operator)
    } else {
        None
    }
}

/// The length of the string at the start of `rest`, quotes included; to the end of the line
/// when it does not close there.
fn string_len(rest: &str, quote: char, escape: Option<char>) -> usize {
    let mut chars = rest.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        if Some(c) == escape {
            chars.next();
        } else if c == quote {
            return at + c.len_utf8();
        }
    }
    rest.len()
}

/// Whether `rest`, which starts with `'`, is a character (`'x'`, `'\n'`, `'\u{1F600}'`) rather
/// than a lifetime.
fn is_char_literal(rest: &str) -> bool {
    let mut chars = rest.chars().skip(1);
    match chars.next() {
        Some('\\') => rest.get(1..16).is_some_and(|tail| tail.contains('\'')),
        Some(_) => chars.next() == Some('\''),
        None => false,
    }
}

fn name_len(rest: &str, dashes: bool) -> usize {
    rest.char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || (dashes && *c == '-')))
        .map_or(rest.len(), |(at, _)| at)
}

/// A number: digits, letters (hex, suffixes, exponents), `_`, and a `.` with a digit after it.
fn number_len(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let mut len = 0;
    while let Some(&b) = bytes.get(len) {
        let fraction = b == b'.' && bytes.get(len + 1).is_some_and(u8::is_ascii_digit);
        if !(b.is_ascii_alphanumeric() || b == b'_' || fraction) {
            break;
        }
        len += 1;
    }
    len.max(1)
}

/// `$name`, `${...}` or a lone `$`.
fn variable_len(rest: &str) -> usize {
    let tail = &rest[1..];
    if tail.starts_with('{') {
        return tail.find('}').map_or(rest.len(), |end| end + 2);
    }
    1 + name_len(tail, false)
}

/// A unified diff's line: added, removed, a hunk header or a file header.
fn diff_line(line: &str) -> LineTokens {
    let headers = ["+++", "---", "diff ", "index "];
    let class = if headers.iter().any(|header| line.starts_with(header)) {
        TokenClass::Comment
    } else if line.starts_with("@@") {
        TokenClass::Keyword
    } else if line.starts_with('+') {
        TokenClass::Number
    } else if line.starts_with('-') {
        TokenClass::Invalid
    } else {
        return LineTokens::new();
    };
    vec![(0..line.len(), class)]
}

fn push(out: &mut LineTokens, range: Range<usize>, class: TokenClass) {
    if !range.is_empty() {
        out.push((range, class));
    }
}

/// The tokens of every code block of `items`, numbered in the order iced's `markdown` view
/// draws them, as the preview counts them (`markdown_view.rs`).
pub fn colours(items: &[Item]) -> Vec<BlockTokens> {
    let collector = Collector::default();
    let _: Element<'_, ()> =
        markdown::view_with(items, Settings::from(&iced::Theme::Light), &collector);
    collector.blocks.into_inner()
}

/// A viewer that draws nothing and lexes each code block.
#[derive(Default)]
struct Collector {
    blocks: RefCell<Vec<BlockTokens>>,
}

impl<'a> Viewer<'a, ()> for Collector {
    fn on_link_click(_url: Uri) {}

    fn code_block(
        &self,
        settings: Settings,
        language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [Text],
    ) -> Element<'a, ()> {
        let tokens = language.and_then(lang_of).and_then(|lang| {
            let texts: Vec<String> = lines
                .iter()
                .map(|line| {
                    line.spans(settings.style)
                        .iter()
                        .map(|span| span.text.as_ref())
                        .collect()
                })
                .collect();
            lex_block(lang, &texts)
        });
        self.blocks.borrow_mut().push(tokens.map(Arc::from));
        Column::new().into()
    }
}

#[cfg(test)]
#[path = "code_lexer_tests.rs"]
mod tests;
