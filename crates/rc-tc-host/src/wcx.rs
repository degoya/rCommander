//! The portable half of TC's packer interface (`wcxhead.h`): its constants, its error codes, the
//! DOS time it reports, and the check of the names it reports, which are outside data.

/// `PK_OM_LIST`: an archive opened to list it.
pub const PK_OM_LIST: i32 = 0;
/// `PK_OM_EXTRACT`: an archive opened to extract from it.
pub const PK_OM_EXTRACT: i32 = 1;
/// `PK_SKIP`: the entry the header named is passed over.
pub const PK_SKIP: i32 = 0;
/// `PK_EXTRACT`: the entry the header named is written to the given path.
pub const PK_EXTRACT: i32 = 2;
/// `E_END_ARCHIVE`: `ReadHeaderExW` has no more entries; not an error.
pub const E_END_ARCHIVE: i32 = 10;
/// The DOS attribute of a directory.
pub const ATTR_DIRECTORY: u32 = 0x10;
/// The longest entry name `tHeaderDataExW` carries, in UTF-16 units without the NUL.
pub const MAX_NAME: usize = 1023;

/// The name of a WCX error code, for a message.
pub fn error_name(code: i32) -> String {
    let name = match code {
        10 => "E_END_ARCHIVE",
        11 => "E_NO_MEMORY",
        12 => "E_BAD_DATA",
        13 => "E_BAD_ARCHIVE",
        14 => "E_UNKNOWN_FORMAT",
        15 => "E_EOPEN",
        16 => "E_ECREATE",
        17 => "E_ECLOSE",
        18 => "E_EREAD",
        19 => "E_EWRITE",
        20 => "E_SMALL_BUF",
        21 => "E_EABORTED",
        22 => "E_NO_FILES",
        23 => "E_TOO_MANY_FILES",
        24 => "E_NOT_SUPPORTED",
        other => return format!("error {other}"),
    };
    name.to_owned()
}

/// A size from the low and high halves of `tHeaderDataExW`.
pub fn size(low: u32, high: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// A DOS date and time (`(year - 1980) << 25 | month << 21 | day << 16 | hour << 11 | minute << 5
/// | second / 2`) as seconds since 1970-01-01 of the same wall clock - DOS times carry no zone, so
/// the caller applies the local one. `None` for a field out of range.
pub fn dos_time(packed: i32) -> Option<i64> {
    let packed = packed as u32;
    let year = i64::from(packed >> 25) + 1980;
    let month = (packed >> 21) & 0x0f;
    let day = (packed >> 16) & 0x1f;
    let hour = (packed >> 11) & 0x1f;
    let minute = (packed >> 5) & 0x3f;
    let second = (packed & 0x1f) * 2;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    if second > 59 {
        return None;
    }
    let days = days_from_civil(year, i64::from(month), i64::from(day));
    Some(days * 86_400 + i64::from(hour * 3600 + minute * 60 + second))
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Whether an entry name a DLL reported may name a file below an extraction folder: relative, no
/// `..` component, no drive or stream (`:`), no NUL, within [`MAX_NAME`]. rCommander drops the
/// others from a listing and counts them, as its own archive backend does with such names.
pub fn safe_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name.encode_utf16().count() <= MAX_NAME
        && !name.contains(['\0', ':'])
        && !name.starts_with(['\\', '/'])
        && name
            .split(['\\', '/'])
            .all(|part| part != ".." && part != ".")
}

#[cfg(test)]
#[path = "wcx_tests.rs"]
mod tests;
