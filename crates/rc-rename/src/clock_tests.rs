use chrono::{NaiveDate, NaiveDateTime};

use super::{OffsetUnit, TimeOffset, local, shifted};

fn at(year: i32, month: u32, day: u32, hour: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|date| date.and_hms_opt(hour, 4, 5))
        .expect("a valid time")
}

#[test]
fn offsets_move_the_time_in_every_unit() {
    let start = at(2024, 1, 31, 23);
    let by = |amount, unit| shifted(start, Some(TimeOffset { amount, unit }));
    assert_eq!(shifted(start, None), Some(start));
    assert_eq!(by(3600, OffsetUnit::Seconds), Some(at(2024, 2, 1, 0)));
    assert_eq!(by(-60, OffsetUnit::Minutes), Some(at(2024, 1, 31, 22)));
    assert_eq!(by(2, OffsetUnit::Hours), Some(at(2024, 2, 1, 1)));
    assert_eq!(by(-31, OffsetUnit::Days), Some(at(2023, 12, 31, 23)));
    // A month later than 31 January is the last day of February.
    assert_eq!(by(1, OffsetUnit::Months), Some(at(2024, 2, 29, 23)));
    assert_eq!(by(-2, OffsetUnit::Years), Some(at(2022, 1, 31, 23)));
    // Beyond the calendar there is no time.
    assert_eq!(by(1_000_000, OffsetUnit::Years), None);
}

#[test]
fn a_picture_formats_and_quotes() {
    let time = at(2024, 3, 9, 14);
    let cases = [
        ("dd.MM.yyyy", "09.03.2024"),
        ("d/M/yy", "9-3-24"),
        ("yyyy-MM-dd", "2024-03-09"),
        ("dddd, MMMM d", "Saturday, March 9"),
        ("HH:mm:ss", "14.04.05"),
        ("h:mm tt", "2.04 PM"),
        ("'Day' d", "Day 9"),
        ("d 'o''clock'", "9 o'clock"),
        ("''d", "'9"),
        ("gg yyyy", " 2024"),
        ("'open", "open"),
    ];
    for (format, expected) in cases {
        assert_eq!(local(format, time), expected, "{format}");
    }
}
