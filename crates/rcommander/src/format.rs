//! How numbers, sizes and dates read in the panels: counts grouped and sizes in bytes, KiB, MiB
//! and up with the marks of the interface's language (RC-110-83), dates in the order and clock
//! of the system's locale, Unix rights as `ls -l` prints them; a size typed into a field read
//! back as bytes.

use std::{sync::OnceLock, time::SystemTime};

use jiff::{
    Timestamp,
    civil::DateTime,
    tz::{AmbiguousOffset, TimeZone},
};
use rc_engine::{Entry, EntryKind};

use crate::{
    i18n::t,
    locale::{self, Lang},
};

/// The decimal mark and the group separator of `lang`: `1,234.5` in English, `1.234,5` in
/// German and Spanish, `1 234,5` in French with U+202F NARROW NO-BREAK SPACE between the groups.
fn marks(lang: Lang) -> (char, char) {
    match lang {
        Lang::En => ('.', ','),
        Lang::De | Lang::Es => (',', '.'),
        Lang::Fr => (',', '\u{202f}'),
    }
}

/// `1234567` grouped by three as the interface's language writes it: `1,234,567` in English,
/// `1.234.567` in German and Spanish, `1 234 567` in French.
pub fn count(number: u64) -> String {
    count_in(locale::current(), number)
}

/// [`count`] in `lang`.
pub(crate) fn count_in(lang: Lang, number: u64) -> String {
    let (_, group) = marks(lang);
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(group);
        }
        out.push(digit);
    }
    out
}

/// A share of the work done, `0.42` reading `42 %`, as the progress bars show it beside them.
pub fn percent(share: f32) -> String {
    format!("{:.0} %", share * 100.0)
}

/// A size in the largest binary unit that keeps it at 1 or more: `512 B`, `1.5 KiB`, `23 MiB`;
/// one decimal below 10, with the decimal mark of the interface's language (`1,5 KiB` in
/// German). The whole part stays ungrouped: below 1024 in every unit but the last.
pub fn size(bytes: u64) -> String {
    let (unit, value) = scaled(locale::current(), bytes);
    match unit {
        0 => t!("size.bytes", value = value),
        1 => t!("size.kib", value = value),
        2 => t!("size.mib", value = value),
        3 => t!("size.gib", value = value),
        _ => t!("size.tib", value = value),
    }
}

/// The unit of a size (0 bytes, 1 KiB, up to 4 TiB) and its number as `lang` writes it.
fn scaled(lang: Lang, bytes: u64) -> (usize, String) {
    if bytes < 1024 {
        return (0, bytes.to_string());
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 1;
    while value >= 1024.0 && unit < 4 {
        value /= 1024.0;
        unit += 1;
    }
    let text = if value < 10.0 {
        format!("{value:.1}").replace('.', &marks(lang).0.to_string())
    } else {
        format!("{value:.0}")
    };
    (unit, text)
}

/// The modification time in local time, as the system's locale writes it; nothing when the file
/// system did not say.
pub fn date(modified: Option<SystemTime>) -> String {
    date_in(modified, pattern())
}

/// An instant in local time, as [`date`] writes it: the update page's last check, a
/// certificate's validity (RC-110-83).
pub fn instant(at: Timestamp) -> String {
    stamp_in(at, pattern())
}

/// The day of an instant in local time, as [`date`] writes it without the time: the day an
/// update was released.
pub fn day(at: Timestamp) -> String {
    stamp_in(at, day_pattern(pattern()))
}

/// The system locale's date pattern, read once.
fn pattern() -> &'static str {
    static PATTERN: OnceLock<&'static str> = OnceLock::new();
    PATTERN.get_or_init(|| date_pattern(locale_tag().as_deref()))
}

/// The day of a [`date_pattern`]: the pattern up to its time, which follows the first space.
fn day_pattern(pattern: &str) -> &str {
    pattern.split_once(' ').map_or(pattern, |(day, _)| day)
}

/// `modified` in local time, written with the `strftime` `pattern`; nothing for a time beyond
/// the years -9999 to 9999 or a pattern that does not write.
pub fn date_in(modified: Option<SystemTime>, pattern: &str) -> String {
    modified
        .and_then(|time| Timestamp::try_from(time).ok())
        .map(|time| stamp_in(time, pattern))
        .unwrap_or_default()
}

/// `at` in local time, written with the `strftime` `pattern`; nothing for a pattern that does
/// not write.
fn stamp_in(at: Timestamp, pattern: &str) -> String {
    jiff::fmt::strtime::format(pattern, TimeZone::system().to_datetime(at)).unwrap_or_default()
}

/// A local date and time as an instant: the earlier of two when the clock went back, none when
/// it skipped the hour.
pub fn from_local(time: DateTime) -> Option<Timestamp> {
    let ambiguous = TimeZone::system().to_ambiguous_timestamp(time);
    match ambiguous.offset() {
        AmbiguousOffset::Gap { .. } => None,
        _ => ambiguous.earlier().ok(),
    }
}

/// The locale dates follow: the POSIX variables for time first, then the system's.
fn locale_tag() -> Option<String> {
    ["LC_ALL", "LC_TIME", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .or_else(sys_locale::get_locale)
}

/// The date pattern of a locale tag such as `de_DE.UTF-8` or `en-US`: day, month and year in
/// the order and with the separator the language uses, minutes on a 24-hour clock (12 hours in
/// the United States); ISO 8601 for any other and for `C`.
pub fn date_pattern(tag: Option<&str>) -> &'static str {
    let tag = tag.unwrap_or_default();
    let mut parts = tag.split(['_', '-', '.', '@']);
    let language = parts.next().unwrap_or_default().to_ascii_lowercase();
    let region = parts.next().unwrap_or_default().to_ascii_uppercase();
    match (language.as_str(), region.as_str()) {
        ("en", "US") => "%m/%d/%Y %I:%M %p",
        ("en", "GB" | "IE" | "AU" | "NZ" | "IN") => "%d/%m/%Y %H:%M",
        (
            "de" | "ru" | "pl" | "cs" | "sk" | "fi" | "nb" | "no" | "nn" | "da" | "tr" | "uk"
            | "ro" | "et" | "lv" | "sl" | "hr" | "sr" | "bg",
            _,
        ) => "%d.%m.%Y %H:%M",
        ("fr" | "es" | "it" | "pt" | "el" | "ca" | "gl", _) => "%d/%m/%Y %H:%M",
        ("nl", _) => "%d-%m-%Y %H:%M",
        ("ja" | "zh" | "ko", _) => "%Y/%m/%d %H:%M",
        _ => "%Y-%m-%d %H:%M",
    }
}

/// Unix rights as `ls -l` prints them, after the kind: `drwxr-xr-x`, `lrwxrwxrwx`,
/// `-rw-r--r--`; set-id and sticky bits as `s`, `S`, `t`, `T`. Empty without a mode.
pub fn rights(kind: &EntryKind, mode: Option<u32>) -> String {
    let Some(mode) = mode else {
        return String::new();
    };
    let mut text = String::with_capacity(10);
    text.push(match kind {
        EntryKind::Dir => 'd',
        EntryKind::Symlink { .. } => 'l',
        EntryKind::File => '-',
        EntryKind::Other => '?',
    });
    // Each triple: read, write, and execute with its special bit (set-uid, set-gid, sticky).
    for (shift, special, mark) in [(6, 0o4000, 's'), (3, 0o2000, 's'), (0, 0o1000, 't')] {
        let bits = (mode >> shift) & 0o7;
        text.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        text.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        text.push(match (bits & 0o1 != 0, mode & special != 0) {
            (true, true) => mark,
            (false, true) => mark.to_ascii_uppercase(),
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    text
}

/// An entry's attributes as a column set's `attributes` column shows them (RC-100-90): the
/// Windows flags as Total Commander writes them, `r` read-only, `a` archive, `h` hidden, `s`
/// system, `-` for one not set; off Windows the rights as [`rights`] writes them.
pub fn attributes(entry: &Entry) -> String {
    let Some(flags) = entry.attributes.windows else {
        return rights(&entry.kind, entry.attributes.mode);
    };
    [
        (entry.attributes.readonly, 'r'),
        (flags.archive, 'a'),
        (flags.hidden, 'h'),
        (flags.system, 's'),
    ]
    .into_iter()
    .map(|(set, mark)| if set { mark } else { '-' })
    .collect()
}

/// The bytes `text` names, as Total Commander takes a size: bytes, or a number with `k`, `m` or
/// `g` (`kb`, `KiB`, ...) in units of 1024 - `4294967295`, `700 MB`, `1.5G`, `100 KiB`. Case does
/// not matter, a comma is a decimal point, a fraction of a byte is rounded down. `None` for
/// anything else; a caller that needs at least one byte checks for it.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(end);
    let unit = unit.trim();
    let unit = unit
        .strip_suffix("ib")
        .or_else(|| unit.strip_suffix('b'))
        .unwrap_or(unit);
    let factor: u64 = match unit {
        "" => 1,
        "k" => 1 << 10,
        "m" => 1 << 20,
        "g" => 1 << 30,
        _ => return None,
    };
    let number = number.replace(',', ".");
    match number.split_once('.') {
        None => number.parse::<u64>().ok()?.checked_mul(factor),
        Some((whole, fraction)) => {
            let whole: u64 = if whole.is_empty() {
                0
            } else {
                whole.parse().ok()?
            };
            if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            // The fraction's bytes, rounded down: exact for what anyone types.
            let digits = fraction.len().min(9);
            let scale = 10_u64.pow(digits as u32);
            let fraction: u64 = fraction[..digits].parse().ok()?;
            let part = u128::from(fraction) * u128::from(factor) / u128::from(scale);
            whole
                .checked_mul(factor)?
                .checked_add(u64::try_from(part).ok()?)
        }
    }
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
