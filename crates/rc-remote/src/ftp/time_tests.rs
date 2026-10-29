use std::time::{Duration, SystemTime};

use super::{Civil, format_stamp, month_number, parse_stamp};

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn stamps_parse_as_utc() {
    assert_eq!(parse_stamp("19700101000000"), Some(at(0)));
    // `date -u -d @1600000000` is 2020-09-13 12:26:40.
    assert_eq!(parse_stamp("20200913122640"), Some(at(1_600_000_000)));
    assert_eq!(parse_stamp("20200913122640.123"), Some(at(1_600_000_000)));
    // A leap day.
    assert_eq!(parse_stamp("20240229000000"), Some(at(1_709_164_800)));
}

#[test]
fn malformed_stamps_are_none() {
    for text in [
        "",
        "2020091312264",
        "2020-09-13 12:26",
        "20201313000000",
        "19691231235959",
    ] {
        assert_eq!(parse_stamp(text), None, "{text}");
    }
}

#[test]
fn formatting_round_trips() {
    for seconds in [0, 951_782_400, 1_600_000_000, 4_102_444_799] {
        let text = format_stamp(at(seconds)).expect("after 1970");
        assert_eq!(parse_stamp(&text), Some(at(seconds)), "{text}");
    }
    assert_eq!(
        format_stamp(at(1_600_000_000)).as_deref(),
        Some("20200913122640")
    );
}

#[test]
fn civil_dates_come_back() {
    let civil = Civil::from_system_time(at(1_709_164_800 + 3661)).expect("civil");
    assert_eq!(
        civil,
        Civil {
            year: 2024,
            month: 2,
            day: 29,
            hour: 1,
            minute: 1,
            second: 1
        }
    );
}

#[test]
fn months_are_english_abbreviations() {
    assert_eq!(month_number("Jan"), Some(1));
    assert_eq!(month_number("DEC"), Some(12));
    assert_eq!(month_number("M\u{e4}r"), None);
}
