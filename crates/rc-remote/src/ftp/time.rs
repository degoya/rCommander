//! Calendar arithmetic for FTP time stamps, without a date library.
//!
//! `MLSD`, `MLST`, `MDTM` and `MFMT` speak `YYYYMMDDHHMMSS[.sss]` in UTC; `LIST` speaks
//! `Mon DD HH:MM` or `Mon DD YYYY` in whatever zone the server runs, taken as UTC here. The
//! civil-date conversions are Howard Hinnant's `days_from_civil` and `civil_from_days`.

use std::time::SystemTime;

use rc_vfs::{unix_seconds, unix_time};

/// A calendar date and time of day, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    // Both are in range by construction: day 1..=31, month 1..=12.
    (year, month as u32, day as u32)
}

impl Civil {
    /// Whether every field is in its calendar range (the day only up to 31).
    fn is_valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && (1..=31).contains(&self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 61
    }

    /// The instant, or `None` for an invalid date or one before 1970.
    pub fn to_system_time(self) -> Option<SystemTime> {
        if !self.is_valid() {
            return None;
        }
        let seconds = days_from_civil(self.year, self.month, self.day) * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second);
        unix_time(u64::try_from(seconds).ok()?)
    }

    /// The calendar date of an instant; `None` before 1970.
    pub fn from_system_time(time: SystemTime) -> Option<Self> {
        let seconds = i64::try_from(unix_seconds(time)?).ok()?;
        let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
        let rest = seconds.rem_euclid(86_400);
        Some(Self {
            year,
            month,
            day,
            // All below 86 400 / 3600 = 24 and 60.
            hour: (rest / 3600) as u32,
            minute: (rest / 60 % 60) as u32,
            second: (rest % 60) as u32,
        })
    }
}

/// Parses `YYYYMMDDHHMMSS`, ignoring fractional seconds (`.sss`) after it.
pub(crate) fn parse_stamp(text: &str) -> Option<SystemTime> {
    let digits = text.trim().split('.').next()?;
    if digits.len() != 14 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let field = |range: std::ops::Range<usize>| digits[range].parse::<u32>().ok();
    Civil {
        year: i64::from(field(0..4)?),
        month: field(4..6)?,
        day: field(6..8)?,
        hour: field(8..10)?,
        minute: field(10..12)?,
        second: field(12..14)?,
    }
    .to_system_time()
}

/// Formats an instant as `YYYYMMDDHHMMSS` (whole seconds, UTC); `None` before 1970.
pub(crate) fn format_stamp(time: SystemTime) -> Option<String> {
    let civil = Civil::from_system_time(time)?;
    Some(format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        civil.year, civil.month, civil.day, civil.hour, civil.minute, civil.second
    ))
}

/// The month number (1..=12) of an English three-letter abbreviation, in any case.
pub(crate) fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = name.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|month| *month == lower)
        .and_then(|index| u32::try_from(index + 1).ok())
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
