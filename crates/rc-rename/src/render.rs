//! A parsed mask applied to one entry.

use chrono::{Datelike, NaiveDateTime, Timelike};

use crate::case::{CaseMode, lower_small_words};
use crate::clock::{self, TimeSource, TimeSwitch};
use crate::counter;
use crate::pattern::{DatePart, Pattern, Range, Token};
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
    /// The folders of the entry's directory from the root, without a drive ([`path_levels`]):
    /// the last is the parent.
    pub levels: &'a [&'a str],
    /// The path from the root for `[B+n]`: the drive or server first ([`from_root`]).
    pub from_root: &'a [&'a str],
    /// The size in bytes, if the listing has it.
    pub size: Option<u64>,
    /// The entry is a folder.
    pub is_dir: bool,
    /// The entry's position in the plan, from 0: what the counter counts.
    pub index: u64,
    /// The number of entries in the plan, for `[C:a]`.
    pub count: u64,
    /// The rule's counter settings.
    pub counter: CounterSettings,
    /// The moments the date placeholders read.
    pub times: Times,
    /// The clipboard's text as `[X]` inserts it; empty when there is none.
    pub clipboard: &'a str,
    /// The local date format of `[d]`.
    pub date_format: &'a str,
    /// The local time format of `[t]`.
    pub time_format: &'a str,
}

/// The three moments of `[T1]`, `[T2]` and `[T3]`, local.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Times {
    /// The entry's modification time.
    pub modified: Option<NaiveDateTime>,
    /// The time the dialog was opened.
    pub opened: Option<NaiveDateTime>,
    /// The time the options last changed.
    pub changed: Option<NaiveDateTime>,
}

/// The switches a mask sets for what follows, in the extension mask too: `[I]` and `[T...]`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Switches {
    ignore_folder_dots: bool,
    time: TimeSwitch,
}

/// A mask with a date placeholder for an entry without the time it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoTimestamp;

/// `pattern` rendered for one entry, after the switches `switches` holds; leaves them as the
/// mask set them.
pub(crate) fn render(
    pattern: &Pattern,
    fields: &Fields<'_>,
    switches: &mut Switches,
) -> Result<String, NoTimestamp> {
    let mut out = String::new();
    let mut case = CaseMode::Unchanged;
    let mut titled = Vec::new();
    for token in pattern.tokens() {
        let whole_folder = switches.ignore_folder_dots && fields.is_dir;
        let piece = match token {
            Token::Case(mode) => {
                case = *mode;
                continue;
            }
            Token::IgnoreFolderDots(on) => {
                switches.ignore_folder_dots = *on;
                continue;
            }
            Token::Time(switch) => {
                switches.time = *switch;
                continue;
            }
            Token::Literal(text) => text.clone(),
            Token::Name(range) if whole_folder => slice(fields.file_name, *range),
            Token::Name(range) => slice(fields.name, *range),
            Token::Extension(_) if whole_folder => String::new(),
            Token::Extension(range) => slice(fields.extension, *range),
            Token::FileName(range) => slice(fields.file_name, *range),
            Token::Parent(range) => slice(ancestor(fields.levels, 0), *range),
            Token::Grandparent(range) => slice(ancestor(fields.levels, 1), *range),
            Token::Ancestor(level, range) => slice(ancestor(fields.levels, *level), *range),
            Token::FromRoot(level, range) => {
                let part = fields.from_root.get(usize::from(*level)).copied();
                slice(part.unwrap_or_default(), *range)
            }
            Token::Counter(spec) => counter::number(*spec, fields),
            Token::Letters(spec) => counter::letters(*spec, fields),
            Token::Size if fields.is_dir => "0".to_owned(),
            Token::Size => fields.size.unwrap_or_default().to_string(),
            Token::Clipboard { .. } => fields.clipboard.to_owned(),
            Token::Date(part) => date(*part, moment(fields, switches.time)?),
            Token::LocalDate => clock::local(fields.date_format, moment(fields, switches.time)?),
            Token::LocalTime => clock::local(fields.time_format, moment(fields, switches.time)?),
        };
        let start = out.len();
        case.append(&mut out, &piece);
        if case == CaseMode::Title {
            titled.push((start, out.len()));
        }
    }
    lower_small_words(&mut out, &titled);
    Ok(out)
}

/// The time a date placeholder reads after `switch`.
fn moment(fields: &Fields<'_>, switch: TimeSwitch) -> Result<NaiveDateTime, NoTimestamp> {
    let time = match switch.source {
        TimeSource::Modified => fields.times.modified,
        TimeSource::Opened => fields.times.opened,
        TimeSource::Changed => fields.times.changed,
    };
    time.and_then(|time| clock::shifted(time, switch.offset))
        .ok_or(NoTimestamp)
}

/// The folder `level` levels above the parent (0 the parent); empty above the root.
fn ancestor<'a>(levels: &[&'a str], level: u8) -> &'a str {
    levels
        .len()
        .checked_sub(usize::from(level) + 1)
        .and_then(|index| levels.get(index))
        .copied()
        .unwrap_or_default()
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

/// The path of `dir` as `[B+n]` counts it: the drive letter (`C`), or for a UNC path the
/// server, then the folders; a path without either (`/home/me`) starts with an empty part, so
/// `[B+1]` is always the first folder.
pub(crate) fn from_root(dir: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = dir
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != "?" && *part != ".")
        .collect();
    let verbatim = dir.starts_with(r"\\?\") || dir.starts_with("//?/");
    if let Some(first) = parts.first_mut().filter(|first| first.ends_with(':')) {
        let drive: &str = first;
        *first = drive.trim_end_matches(':');
        return parts;
    }
    // `\\?\UNC\server\share`: the server first, as in `\\server\share`.
    if verbatim && parts.first() == Some(&"UNC") {
        parts.remove(0);
        return parts;
    }
    if !verbatim && (dir.starts_with(r"\\") || dir.starts_with("//")) {
        return parts;
    }
    parts.insert(0, "");
    parts
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
