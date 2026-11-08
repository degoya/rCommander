//! To-do lists in TaskPaper's and PlainTasks' plain text (`.todo`, `.taskpaper`, RC-110-07): a
//! project is a line ending in `:` (tags may follow it), a task a line starting with a mark - open
//! (`-`, `*`, `☐`, `□`, `❍`, `❑`, `[ ]`), done (`✔`, `✓`, `☑`, `+`, `[x]`) or cancelled (`✘`,
//! `✗`) - and every other line a note. Tags `@name` and `@name(value)` stand anywhere after a
//! blank; the date tags (`@done(26-05-10 14:15)`, `@due(...)` and their kin) are coloured apart
//! from the others, and a value that starts with a digit - a date, a time, a number - apart from
//! one that does not. A task that is done or cancelled, by its mark or by `@done`/`@cancelled`,
//! is dimmed: its text a comment, its tags still coloured. PlainTasks' archive rule (`--- ✄ ---`,
//! `＿＿＿`) is a comment. Nothing reaches from one line into the next.

use super::{Cursor, Emit, MAX_INLINE};
use crate::types::TokenClass;

/// The marks of an open task.
const OPEN: &[&[u8]] = &[
    b"-",
    b"*",
    "☐".as_bytes(),
    "□".as_bytes(),
    "❍".as_bytes(),
    "❑".as_bytes(),
    b"[ ]",
];
/// The marks of a task done.
const DONE: &[&[u8]] = &[
    "✔".as_bytes(),
    "✓".as_bytes(),
    "☑".as_bytes(),
    b"+",
    b"[x]",
    b"[X]",
];
/// The marks of a task cancelled.
const CANCELLED: &[&[u8]] = &["✘".as_bytes(), "✗".as_bytes()];
/// Tags whose value is a date or a time: PlainTasks' and TaskPaper's.
const DATE_TAGS: &[&[u8]] = &[
    b"done",
    b"started",
    b"due",
    b"created",
    b"cancelled",
    b"lasted",
    b"wasted",
    b"estimated",
    b"today",
];
/// Tags that close a task: it shows dimmed.
const CLOSING_TAGS: &[&[u8]] = &[b"done", b"cancelled"];

/// What a task's mark says.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Open,
    Done,
    Cancelled,
}

/// A tag in a line: `@` at `start`, its name up to `name_end`, and the bytes of `(` and `)` of
/// its value when it has one.
struct Tag {
    start: usize,
    name_end: usize,
    value: Option<(usize, usize)>,
}

/// Lexes one line of a to-do list.
pub(crate) fn line<E: Emit>(cur: &mut Cursor<'_, E>) {
    let line = cur.line;
    let indent = cur.blank_end(0);
    if indent == line.len() {
        return;
    }
    let content = &line[indent..];
    if content.starts_with(b"---") || content.starts_with("＿＿".as_bytes()) {
        cur.take(indent, line.len(), TokenClass::Comment);
        return;
    }
    if let Some((mark, end)) = mark(line, indent) {
        let class = match mark {
            Mark::Open => TokenClass::Operator,
            Mark::Done => TokenClass::Literal,
            Mark::Cancelled => TokenClass::Invalid,
        };
        cur.take(indent, end, class);
        let closed = mark != Mark::Open
            || tags(line, end)
                .any(|tag| CLOSING_TAGS.contains(&&line[tag.start + 1..tag.name_end]));
        body(cur, end, closed);
        return;
    }
    let text_end = tags(line, indent)
        .next()
        .map_or(line.len(), |tag| tag.start);
    let head = trim_end(line, indent, text_end);
    if head > indent + 1 && line[head - 1] == b':' {
        cur.take(indent, head - 1, TokenClass::Keyword);
        cur.take(head - 1, head, TokenClass::Punctuation);
    }
    body(cur, cur.pos.max(indent), false);
}

/// The mark of a task at `at` and where it ends, when the line starts with one followed by a
/// blank or the line's end.
fn mark(line: &[u8], at: usize) -> Option<(Mark, usize)> {
    let rest = &line[at..];
    [
        (OPEN, Mark::Open),
        (DONE, Mark::Done),
        (CANCELLED, Mark::Cancelled),
    ]
    .into_iter()
    .find_map(|(marks, mark)| {
        let found = marks.iter().find(|candidate| rest.starts_with(candidate))?;
        let end = at + found.len();
        matches!(line.get(end), None | Some(b' ' | b'\t')).then_some((mark, end))
    })
}

/// The text and tags from `from` on: each tag coloured, the text between them a comment when
/// the task is `closed`, else left as it is.
fn body<E: Emit>(cur: &mut Cursor<'_, E>, from: usize, closed: bool) {
    let line = cur.line;
    let mut text = from;
    for tag in tags(line, from) {
        if closed {
            text_part(cur, text, tag.start);
        }
        let name = &line[tag.start + 1..tag.name_end];
        let class = if DATE_TAGS.contains(&name) {
            TokenClass::TypeName
        } else {
            TokenClass::Function
        };
        cur.take(tag.start, tag.name_end, class);
        text = tag.name_end;
        if let Some((open, close)) = tag.value {
            cur.take(open, open + 1, TokenClass::Punctuation);
            let value_class = if line[cur.blank_end(open + 1)].is_ascii_digit() {
                TokenClass::Number
            } else {
                TokenClass::String
            };
            cur.take(open + 1, close, value_class);
            cur.take(close, close + 1, TokenClass::Punctuation);
            text = close + 1;
        }
    }
    if closed {
        text_part(cur, text, line.len());
    }
    cur.pos = line.len();
}

/// The text `start..end` of a closed task, without its blanks at either end, as a comment.
fn text_part<E: Emit>(cur: &mut Cursor<'_, E>, start: usize, end: usize) {
    let start = cur.blank_end(start).min(end);
    let end = trim_end(cur.line, start, end);
    cur.push(start, end, TokenClass::Comment);
}

/// The tags of `line` from `from` on, in order: an `@` at `from` or after a blank, a name of
/// letters, digits, `_` and `-` (or any non-ASCII character), and a value in parentheses right
/// after it that closes within [`MAX_INLINE`] bytes, before any other `(`.
fn tags(line: &[u8], from: usize) -> impl Iterator<Item = Tag> + '_ {
    let mut at = from;
    std::iter::from_fn(move || {
        while at < line.len() {
            let start = at;
            at += 1;
            if line[start] != b'@' || (start > from && !matches!(line[start - 1], b' ' | b'\t')) {
                continue;
            }
            let mut name_end = start + 1;
            while line.get(name_end).is_some_and(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') || *byte >= 0x80
            }) {
                name_end += 1;
            }
            if name_end == start + 1 {
                continue;
            }
            let value = if line.get(name_end) == Some(&b'(') {
                let limit = (name_end + 1 + MAX_INLINE).min(line.len());
                (name_end + 1..limit)
                    .find(|&at| matches!(line[at], b'(' | b')'))
                    .filter(|&close| line[close] == b')')
                    .map(|close| (name_end, close))
            } else {
                None
            };
            at = value.map_or(name_end, |(_, close)| close + 1);
            return Some(Tag {
                start,
                name_end,
                value,
            });
        }
        None
    })
}

/// `end` moved back over the blanks after `start`.
fn trim_end(line: &[u8], start: usize, end: usize) -> usize {
    let mut end = end;
    while end > start && matches!(line[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    end
}

#[cfg(test)]
#[path = "todo_tests.rs"]
mod tests;
