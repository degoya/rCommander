//! How numbers, sizes and dates read in the panels: counts grouped by a narrow space, sizes in
//! bytes, KiB, MiB and up, dates in the order and clock of the system's locale, Unix rights as
//! `ls -l` prints them; a size typed into a field read back as bytes.

use std::{sync::OnceLock, time::SystemTime};

use chrono::{DateTime, Local};
use rc_engine::EntryKind;

use crate::i18n::t;

/// `1234567` reads `1 234 567`, with U+202F NARROW NO-BREAK SPACE between the groups.
pub fn count(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
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
/// one decimal below 10.
pub fn size(bytes: u64) -> String {
    if bytes < 1024 {
        return t!("size.bytes", value = bytes);
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < 3 {
        value /= 1024.0;
        unit += 1;
    }
    let value = if value < 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    };
    match unit {
        0 => t!("size.kib", value = value),
        1 => t!("size.mib", value = value),
        2 => t!("size.gib", value = value),
        _ => t!("size.tib", value = value),
    }
}

/// The modification time in local time, as the system's locale writes it; nothing when the file
/// system did not say.
pub fn date(modified: Option<SystemTime>) -> String {
    static PATTERN: OnceLock<&'static str> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| date_pattern(locale_tag().as_deref()));
    date_in(modified, pattern)
}

/// `modified` written with the chrono `pattern`.
pub fn date_in(modified: Option<SystemTime>, pattern: &str) -> String {
    modified.map_or_else(String::new, |time| {
        DateTime::<Local>::from(time).format(pattern).to_string()
    })
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
