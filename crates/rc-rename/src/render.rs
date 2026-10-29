//! A parsed mask applied to one entry.

use chrono::{Datelike, NaiveDateTime, Timelike};

use crate::case::CaseMode;
use crate::pattern::{
    CounterSpec, CounterWidth, DatePart, MAX_COUNTER_WIDTH, Pattern, Range, Token,
};
use crate::plan::CounterSettings;

/// What a mask is rendered from for one entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fields<'a> {
    /// The name without the extension.
    pub name: &'a str,
    /// The extension without the dot.
    pub extension: &'a str,
    /// The name with the extension.
    pub file_name: &'a str,
    /// The parent folder's name; empty at the root.
    pub parent: &'a str,
    /// The grandparent folder's name; empty near the root.
    pub grandparent: &'a str,
    /// The entry's position in the plan, from 0: what the counter counts.
    pub index: u64,
    /// The number of entries in the plan, for `[C:a]`.
    pub count: u64,
    /// The rule's counter settings.
    pub counter: CounterSettings,
    /// The modification time, local.
    pub modified: Option<NaiveDateTime>,
}

/// A mask with a date placeholder for an entry without a modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoTimestamp;

/// `pattern` rendered for one entry.
pub(crate) fn render(pattern: &Pattern, fields: &Fields<'_>) -> Result<String, NoTimestamp> {
    let mut out = String::new();
    let mut case = CaseMode::Unchanged;
    for token in pattern.tokens() {
        let piece = match token {
            Token::Case(mode) => {
                case = *mode;
                continue;
            }
            Token::Literal(text) => text.clone(),
            Token::Name(range) => slice(fields.name, *range),
            Token::Extension(range) => slice(fields.extension, *range),
            Token::FileName(range) => slice(fields.file_name, *range),
            Token::Parent(range) => slice(fields.parent, *range),
            Token::Grandparent(range) => slice(fields.grandparent, *range),
            Token::Counter(spec) => counter(*spec, fields),
            Token::Date(part) => date(*part, fields.modified.ok_or(NoTimestamp)?),
        };
        case.append(&mut out, &piece);
    }
    Ok(out)
}

/// The characters of `text` that `range` takes.
pub(crate) fn slice(text: &str, range: Range) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = i64::try_from(chars.len()).unwrap_or(i64::MAX);
    let resolve = |position: i64| {
        if position > 0 {
            position
        } else {
            len.saturating_add(position).saturating_add(1)
        }
    };
    let (from, to) = match range {
        Range::All => return text.to_owned(),
        Range::At(position) => {
            let at = resolve(position);
            (at, at)
        }
        Range::Span(from, to) => (resolve(from), to.map_or(len, resolve)),
        Range::Count(from, count) => {
            let from = resolve(from);
            let span = i64::try_from(count).unwrap_or(i64::MAX).saturating_sub(1);
            (from, from.saturating_add(span))
        }
    };
    let (from, to) = (from.max(1), to.min(len));
    if from > to {
        return String::new();
    }
    let (Ok(from), Ok(to)) = (usize::try_from(from - 1), usize::try_from(to)) else {
        return String::new();
    };
    chars[from..to].iter().collect()
}

/// The folders of `dir` from the root: separated by `/` or `\`, without a drive (`C:`) and
/// the `\\?\` prefix. `\\nas\media\film` is `nas`, `media`, `film`.
pub(crate) fn path_levels(dir: &str) -> Vec<&str> {
    let mut levels: Vec<&str> = dir
        .split(['/', '\\'])
        .filter(|level| !level.is_empty() && *level != "?" && *level != ".")
        .collect();
    if levels.first().is_some_and(|first| first.ends_with(':')) {
        levels.remove(0);
    }
    levels
}

/// The counter for the entry at `index`: start plus index times step, padded with zeros.
pub(crate) fn counter(spec: CounterSpec, fields: &Fields<'_>) -> String {
    let settings = fields.counter;
    let start = i128::from(spec.start.unwrap_or(settings.start));
    let step = i128::from(spec.step.unwrap_or(settings.step));
    let value_at = |index: u64| start + step * i128::from(index);
    let width = match spec.width.unwrap_or(CounterWidth::Digits(settings.width)) {
        CounterWidth::Digits(width) => width.min(MAX_COUNTER_WIDTH),
        CounterWidth::Auto => {
            let last = value_at(fields.count.saturating_sub(1));
            let widest = start.unsigned_abs().max(last.unsigned_abs());
            widest.checked_ilog10().map_or(1, |digits| digits + 1)
        }
    };
    let width = usize::try_from(width).unwrap_or(1);
    let value = value_at(fields.index);
    let digits = format!("{:0width$}", value.unsigned_abs());
    if value < 0 {
        format!("-{digits}")
    } else {
        digits
    }
}

fn date(part: DatePart, time: NaiveDateTime) -> String {
    let (pm, hour12) = time.hour12();
    match part {
        DatePart::Year => format!("{:04}", time.year()),
        DatePart::Year2 => format!("{:02}", time.year().rem_euclid(100)),
        DatePart::Month => format!("{:02}", time.month()),
        DatePart::Day => format!("{:02}", time.day()),
        DatePart::Hour => format!("{:02}", time.hour()),
        DatePart::Hour12 => format!("{hour12:02}"),
        DatePart::Minute => format!("{:02}", time.minute()),
        DatePart::Second => format!("{:02}", time.second()),
        DatePart::AmPm => String::from(if pm { "pm" } else { "am" }),
        DatePart::AmPmShort => String::from(if pm { "p" } else { "a" }),
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
