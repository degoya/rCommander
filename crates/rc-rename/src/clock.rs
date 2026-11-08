//! The times the date placeholders read: which moment (`[T1]`, `[T2]`, `[T3]`, each with an
//! optional offset such as `[T1+60m]`), and the local date and time of `[d]` and `[t]`.

use chrono::{Datelike, Months, NaiveDateTime, TimeDelta, Timelike};

/// Which moment the date placeholders after a `[T...]` read (Total Commander's switches).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TimeSource {
    /// `[T1]`: the entry's modification time, the default.
    #[default]
    Modified,
    /// `[T2]`: the time the rename dialog was opened.
    Opened,
    /// `[T3]`: the time the rename options were last changed.
    Changed,
}

/// The unit of a [`TimeOffset`]: Total Commander's `s`, `m`, `h`, `D`, `M`, `Y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Years,
}

impl OffsetUnit {
    pub(crate) fn from_letter(letter: char) -> Option<Self> {
        Some(match letter {
            's' => Self::Seconds,
            'm' => Self::Minutes,
            'h' => Self::Hours,
            'D' => Self::Days,
            'M' => Self::Months,
            'Y' => Self::Years,
            _ => return None,
        })
    }
}

/// What `[T1+60m]` adds to its moment; `amount` is negative for `-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOffset {
    pub amount: i64,
    pub unit: OffsetUnit,
}

/// A `[T...]` switch: the moment and its offset for every date placeholder after it, in the
/// extension mask too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeSwitch {
    pub source: TimeSource,
    pub offset: Option<TimeOffset>,
}

/// The largest offset a switch takes, in its unit: far beyond any useful one, and no sum of
/// it overflows.
pub(crate) const MAX_OFFSET: i64 = 1_000_000;

/// The local date's format where the window gives none (Linux, macOS, MCP): ISO 8601.
pub const DEFAULT_DATE_FORMAT: &str = "yyyy-MM-dd";
/// The local time's format where the window gives none: 24 hours, as `[t]` writes it
/// `hh.mm.ss`.
pub const DEFAULT_TIME_FORMAT: &str = "HH:mm:ss";
/// The longest date or time format a request may give: a system's pattern is a few characters.
pub const MAX_FORMAT_TEXT: usize = 80;

/// `time` moved by `offset`; `None` when the result lies outside chrono's range.
pub(crate) fn shifted(time: NaiveDateTime, offset: Option<TimeOffset>) -> Option<NaiveDateTime> {
    let Some(TimeOffset { amount, unit }) = offset else {
        return Some(time);
    };
    let seconds = |factor: i64| {
        TimeDelta::try_seconds(amount.checked_mul(factor)?)
            .and_then(|delta| time.checked_add_signed(delta))
    };
    let months = |factor: i64| {
        let months = amount.checked_mul(factor)?;
        let count = Months::new(u32::try_from(months.unsigned_abs()).ok()?);
        if months < 0 {
            time.checked_sub_months(count)
        } else {
            time.checked_add_months(count)
        }
    };
    match unit {
        OffsetUnit::Seconds => seconds(1),
        OffsetUnit::Minutes => seconds(60),
        OffsetUnit::Hours => seconds(3600),
        OffsetUnit::Days => seconds(86_400),
        OffsetUnit::Months => months(1),
        OffsetUnit::Years => months(12),
    }
}

/// `time` in a Windows date or time picture (`dd.MM.yyyy`, `h:mm:ss tt`, as `GetLocaleInfoEx`
/// gives `LOCALE_SSHORTDATE` and `LOCALE_STIMEFORMAT`), then `/` turned into `-` and `:` into
/// `.` as Total Commander's `[d]` and `[t]` do. Text in single quotes is literal (`''` is a
/// quote); day and month names are English; any other character is taken as it is.
pub(crate) fn local(format: &str, time: NaiveDateTime) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let run = chars[at..].iter().take_while(|&&next| next == c).count();
        at += run;
        match c {
            '\'' => at = quoted(&chars, at - run + 1, &mut out),
            'd' => out.push_str(&match run {
                1 => time.day().to_string(),
                2 => format!("{:02}", time.day()),
                3 => short(DAYS[weekday(time)]),
                _ => DAYS[weekday(time)].to_owned(),
            }),
            'M' => out.push_str(&match run {
                1 => time.month().to_string(),
                2 => format!("{:02}", time.month()),
                3 => short(MONTHS[month(time)]),
                _ => MONTHS[month(time)].to_owned(),
            }),
            'y' => out.push_str(&match run {
                1 => time.year().rem_euclid(100).to_string(),
                2 => format!("{:02}", time.year().rem_euclid(100)),
                _ => format!("{:04}", time.year()),
            }),
            'h' | 'H' | 'm' | 's' => {
                let value = match c {
                    'h' => time.hour12().1,
                    'H' => time.hour(),
                    'm' => time.minute(),
                    _ => time.second(),
                };
                out.push_str(&if run == 1 {
                    value.to_string()
                } else {
                    format!("{value:02}")
                });
            }
            't' => {
                let pm = time.hour12().0;
                out.push_str(match (run, pm) {
                    (1, false) => "A",
                    (1, true) => "P",
                    (_, false) => "AM",
                    (_, true) => "PM",
                });
            }
            // The era (`g`, `gg`): nothing in the Gregorian calendar.
            'g' => {}
            other => (0..run).for_each(|_| out.push(other)),
        }
    }
    out.replace('/', "-").replace(':', ".")
}

/// Appends the quoted text that starts at `at` (after its opening quote); where it ends.
fn quoted(chars: &[char], mut at: usize, out: &mut String) -> usize {
    // `''` outside a quoted text is one quote.
    if chars.get(at) == Some(&'\'') {
        out.push('\'');
        return at + 1;
    }
    while let Some(&c) = chars.get(at) {
        at += 1;
        if c != '\'' {
            out.push(c);
        } else if chars.get(at) == Some(&'\'') {
            out.push('\'');
            at += 1;
        } else {
            break;
        }
    }
    at
}

const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn weekday(time: NaiveDateTime) -> usize {
    usize::try_from(time.weekday().num_days_from_monday()).unwrap_or_default()
}

fn month(time: NaiveDateTime) -> usize {
    usize::try_from(time.month0()).unwrap_or_default()
}

fn short(name: &str) -> String {
    name.chars().take(3).collect()
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
