//! How values are written: hex with colons, UTC dates from Unix seconds, and text from a file
//! made safe to show (no control characters, a bounded length).

use std::fmt::Write as _;

/// The most characters a value shows; a longer one ends in an ellipsis.
pub const MAX_VALUE_CHARS: usize = 4096;

/// Seconds in a day.
pub const DAY: i64 = 86_400;

/// Bytes as upper-case hex pairs joined by colons, as OpenSSL shows fingerprints.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push(':');
        }
        // Writing to a String cannot fail.
        let _ = write!(out, "{byte:02X}");
    }
    out
}

/// A Unix time as `YYYY-MM-DD HH:MM:SS UTC` (proleptic Gregorian calendar).
#[must_use]
pub fn date(seconds: i64) -> String {
    let days = seconds.div_euclid(DAY);
    let second_of_day = seconds.rem_euclid(DAY);
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        second_of_day / 3600,
        second_of_day % 3600 / 60,
        second_of_day % 60
    )
}

/// Year, month and day of the day `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days.saturating_add(719_468);
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era
        .saturating_add(era.saturating_mul(400))
        .saturating_add(i64::from(month <= 2));
    (year, month, day)
}

/// Whole days from `now` to `then`, rounded towards the past (negative when `then` is earlier).
#[must_use]
pub fn days_between(now: i64, then: i64) -> i64 {
    then.saturating_sub(now).div_euclid(DAY)
}

/// Text from a file made safe to show: control characters become U+FFFD, and the value stops
/// after [`MAX_VALUE_CHARS`] characters.
#[must_use]
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_VALUE_CHARS));
    for (count, character) in text.chars().enumerate() {
        if count == MAX_VALUE_CHARS {
            out.push_str("...");
            break;
        }
        out.push(if character.is_control() {
            char::REPLACEMENT_CHARACTER
        } else {
            character
        });
    }
    out
}

/// Bytes from a file shown as text: UTF-8 where it is, U+FFFD elsewhere, then [`clean`].
#[must_use]
pub fn clean_bytes(bytes: &[u8]) -> String {
    clean(&String::from_utf8_lossy(bytes))
}

#[cfg(test)]
#[path = "show_tests.rs"]
mod tests;
