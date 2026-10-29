//! What each line of a diff is, from its first bytes and the state the line before left - one
//! classifier for both passes: the line index and the tree of `open`, the tokens of the lines on
//! screen.
//!
//! A unified or combined hunk is counted, not guessed: its header promises so many lines from the
//! parents and for the result, and the hunk ends when they are all read. So a removed line
//! `--- x` inside a hunk stays a removed line, while the same text after the hunk starts the next
//! file. A context diff (`***`/`---`, `!`) is followed by its markers alone. Outside a hunk a line
//! is a file header, one of git's extended headers, or text: a mail's header and message before
//! the first diff, a signature after the last.

/// The most parents a combined diff's hunk header (`@@@ ... @@@`) may name.
const MAX_PARENTS: usize = 64;
/// The line that starts a context diff's hunk.
const CONTEXT_HUNK: &[u8] = b"***************";

/// What a line starts inside of. Small and `Copy`: every checkpoint of the line index keeps one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LineState {
    /// Before the first diff: a mail's header and message, a cover text.
    #[default]
    Preamble,
    /// Between files and hunks: file headers, git's extended headers.
    Files,
    /// In a unified (`parents` 1) or combined hunk: the lines its header still promises from the
    /// parents, summed, and for the result.
    Hunk { parents: u8, old: u32, new: u32 },
    /// In a hunk whose header gave no counts: as long as lines look like its body.
    Loose { parents: u8 },
    /// In a context diff's hunk: its old part (`*** a,b ****`) or its new part (`--- c,d ----`).
    Context { new: bool },
}

/// A file header line, by its syntax; what it means for the file is the tree's to work out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Header {
    /// `--- path`: the old file of a unified diff, the new file of a context diff.
    Minus,
    /// `+++ path`.
    Plus,
    /// `*** path`: the old file of a context diff.
    Star,
    /// `new file mode`.
    NewFile,
    /// `deleted file mode`.
    Deleted,
    /// `rename from`, `rename old`.
    RenameFrom,
    /// `rename to`, `rename new`.
    RenameTo,
    CopyFrom,
    CopyTo,
    /// `Binary files ... differ`, `GIT binary patch`.
    Binary,
    /// `index`, the modes, `similarity index`.
    Other,
}

/// What a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Text outside the diff: a mail header, a message, a diffstat, a signature.
    Text,
    /// `diff ...`: a file starts.
    DiffStart,
    Header(Header),
    /// A hunk header: `@@ ... @@` ends at byte `end`, the function context after it starts at
    /// `context`. Everything before `end` is ASCII.
    HunkHead {
        end: usize,
        context: usize,
    },
    /// A context diff's `***************`.
    ContextHunk,
    /// A context diff's `*** a,b ****` or `--- c,d ----`.
    ContextRange,
    Added,
    Removed,
    Unchanged,
    /// `\ No newline at end of file`.
    NoNewline,
}

/// git's extended header lines and the binary markers.
const GIT_HEADERS: [(&[u8], Header); 16] = [
    (b"index ", Header::Other),
    (b"old mode ", Header::Other),
    (b"new mode ", Header::Other),
    (b"mode ", Header::Other),
    (b"deleted file mode ", Header::Deleted),
    (b"new file mode ", Header::NewFile),
    (b"similarity index ", Header::Other),
    (b"dissimilarity index ", Header::Other),
    (b"rename from ", Header::RenameFrom),
    (b"rename to ", Header::RenameTo),
    (b"rename old ", Header::RenameFrom),
    (b"rename new ", Header::RenameTo),
    (b"copy from ", Header::CopyFrom),
    (b"copy to ", Header::CopyTo),
    (b"Binary files ", Header::Binary),
    (b"GIT binary patch", Header::Binary),
];

/// The kind of `line` - without its line break and a `\r` before it - in `state`, which becomes
/// the state the next line starts in.
pub(crate) fn classify(state: &mut LineState, line: &[u8]) -> Kind {
    match *state {
        LineState::Preamble | LineState::Files => outside(state, line),
        LineState::Hunk { parents, old, new } => {
            if line.first() == Some(&b'\\') {
                return Kind::NoNewline;
            }
            if let Some((kind, from_old, to_new)) = body(parents, line)
                && from_old <= old
                && to_new <= new
            {
                let (old, new) = (old - from_old, new - to_new);
                *state = if old == 0 && new == 0 {
                    LineState::Files
                } else {
                    LineState::Hunk { parents, old, new }
                };
                return kind;
            }
            // A line the hunk did not promise: it ends early.
            *state = LineState::Files;
            outside(state, line)
        }
        LineState::Loose { parents } => {
            if line.first() == Some(&b'\\') {
                return Kind::NoNewline;
            }
            if !line.starts_with(b"@@")
                && !line.starts_with(b"diff ")
                && let Some((kind, _, _)) = body(parents, line)
            {
                return kind;
            }
            *state = LineState::Files;
            outside(state, line)
        }
        LineState::Context { new } => context(state, new, line),
    }
}

/// A line outside any hunk.
fn outside(state: &mut LineState, line: &[u8]) -> Kind {
    let preamble = *state == LineState::Preamble;
    if line.starts_with(b"diff ") {
        *state = LineState::Files;
        return Kind::DiffStart;
    }
    if let Some((next, kind)) = hunk_head(line) {
        *state = next;
        return kind;
    }
    if is_context_hunk(line) {
        *state = LineState::Context { new: false };
        return Kind::ContextHunk;
    }
    if line.starts_with(b"--- ") {
        *state = LineState::Files;
        return Kind::Header(Header::Minus);
    }
    if line.starts_with(b"*** ") {
        *state = LineState::Files;
        return Kind::Header(Header::Star);
    }
    if preamble {
        return Kind::Text;
    }
    if line.starts_with(b"+++ ") {
        return Kind::Header(Header::Plus);
    }
    if line.first() == Some(&b'\\') {
        return Kind::NoNewline;
    }
    GIT_HEADERS
        .iter()
        .find(|(prefix, _)| line.starts_with(prefix))
        .map_or(Kind::Text, |(_, header)| Kind::Header(*header))
}

/// A line of a unified (`parents` 1) or combined hunk's body: its kind, how many parents hold it
/// and whether the result does; `None` for a line no hunk has.
fn body(parents: u8, line: &[u8]) -> Option<(Kind, u32, u32)> {
    let parents = usize::from(parents);
    if line.is_empty() {
        // An unchanged line whose leading blanks an editor or a mail took off.
        return Some((Kind::Unchanged, count(parents), 1));
    }
    let columns = line.get(..parents)?;
    if !columns.iter().all(|byte| b" +-".contains(byte)) {
        return None;
    }
    let of = |wanted: u8| count(columns.iter().filter(|byte| **byte == wanted).count());
    // A `-` in column N: parent N has the line and the result does not. Without one, the line is
    // in the result, and in each parent with a blank in its column.
    Some(if of(b'-') > 0 {
        (Kind::Removed, of(b'-'), 0)
    } else if of(b'+') > 0 {
        (Kind::Added, of(b' '), 1)
    } else {
        (Kind::Unchanged, count(parents), 1)
    })
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A hunk header `@@ -a,b +c,d @@ context` or, for a combined diff of N parents, N + 1 `@`
/// around N old ranges and one new: the state after it and its kind. Counts that do not parse
/// make the hunk loose; no closing `@@` makes the line no header.
fn hunk_head(line: &[u8]) -> Option<(LineState, Kind)> {
    let ats = line.iter().take_while(|byte| **byte == b'@').count();
    if !(2..=MAX_PARENTS + 1).contains(&ats) {
        return None;
    }
    let parents = u8::try_from(ats - 1).ok()?;
    let after = line.get(ats..)?;
    let close = after.windows(ats + 1).position(|window| {
        window.first() == Some(&b' ') && window[1..].iter().all(|b| *b == b'@')
    })?;
    let ranges = after.get(..close)?;
    if !ranges.is_ascii() {
        return None;
    }
    let end = ats + close + 1 + ats;
    let context = if line.get(end) == Some(&b' ') {
        end + 1
    } else {
        end
    };
    let kind = Kind::HunkHead { end, context };
    let state = match counts(parents, ranges) {
        Some((0, 0)) => LineState::Files,
        Some((old, new)) => LineState::Hunk { parents, old, new },
        None => LineState::Loose { parents },
    };
    Some((state, kind))
}

/// The lines ` -a,b -c,d +e,f` promise: from the parents, summed, and for the result.
fn counts(parents: u8, ranges: &[u8]) -> Option<(u32, u32)> {
    let mut words = ranges
        .split(|byte| *byte == b' ')
        .filter(|word| !word.is_empty());
    let mut old = 0u32;
    for _ in 0..parents {
        old = old.saturating_add(range(words.next()?.strip_prefix(b"-")?)?);
    }
    let new = range(words.next()?.strip_prefix(b"+")?)?;
    words.next().is_none().then_some((old, new))
}

/// The line count of `start,count`, or 1 for `start` alone.
fn range(spec: &[u8]) -> Option<u32> {
    let number = |digits: &[u8]| -> Option<u32> {
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(digits).ok()?.parse().ok()
    };
    match spec.iter().position(|byte| *byte == b',') {
        Some(comma) => {
            number(spec.get(..comma)?)?;
            number(spec.get(comma + 1..)?)
        }
        None => number(spec).map(|_| 1),
    }
}

/// A line of a context diff's hunk.
fn context(state: &mut LineState, new: bool, line: &[u8]) -> Kind {
    if is_context_hunk(line) {
        *state = LineState::Context { new: false };
        return Kind::ContextHunk;
    }
    if is_context_range(line, b"*** ", b" ****") {
        *state = LineState::Context { new: false };
        return Kind::ContextRange;
    }
    if is_context_range(line, b"--- ", b" ----") {
        *state = LineState::Context { new: true };
        return Kind::ContextRange;
    }
    match line.get(..2) {
        Some(b"  ") => Kind::Unchanged,
        Some(b"- ") => Kind::Removed,
        Some(b"+ ") => Kind::Added,
        // Changed: the old text in the old part, the new text in the new.
        Some(b"! ") if new => Kind::Added,
        Some(b"! ") => Kind::Removed,
        _ if line.is_empty() => Kind::Unchanged,
        _ if line.first() == Some(&b'\\') => Kind::NoNewline,
        _ => {
            *state = LineState::Files;
            outside(state, line)
        }
    }
}

/// `***************`, as long as it is all stars.
fn is_context_hunk(line: &[u8]) -> bool {
    line.starts_with(CONTEXT_HUNK) && line.iter().all(|byte| *byte == b'*')
}

/// `<open>a,b<close>` or `<open>a<close>`.
fn is_context_range(line: &[u8], open: &[u8], close: &[u8]) -> bool {
    line.strip_prefix(open)
        .and_then(|rest| rest.strip_suffix(close))
        .is_some_and(|middle| range(middle).is_some())
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
