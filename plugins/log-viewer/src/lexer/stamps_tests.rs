use super::*;

/// What `matcher` takes of `text` from its start.
fn taken(text: &str, matcher: fn(&[u8], usize) -> Option<usize>) -> Option<&str> {
    matcher(text.as_bytes(), 0).map(|end| &text[..end])
}

#[test]
fn iso_takes_date_time_fraction_and_zone_and_stops_before_the_rest() {
    for (text, stamp) in [
        ("2026-10-20, x", "2026-10-20"),
        ("2026-10-20 x", "2026-10-20"),
        ("2026-10-20T15:20 x", "2026-10-20T15:20"),
        ("2026-10-20 15:20:01, Info", "2026-10-20 15:20:01"),
        // Ten digits are no fraction: the timestamp ends before them.
        ("2026-10-20 15:20:01.1234567890 x", "2026-10-20 15:20:01"),
        ("2026-10-20T15:20:01+02 x", "2026-10-20T15:20:01+02"),
        ("2026-10-20T15:20:01Z]", "2026-10-20T15:20:01Z"),
    ] {
        assert_eq!(taken(text, at_digit), Some(stamp), "{text}");
    }
    for text in ["2026-10-2", "2026-10/20", "26-10-20", "2026-10-20T"] {
        assert_eq!(taken(text, at_digit), None, "{text}");
    }
}

#[test]
fn line_start_forms() {
    assert_eq!(taken("1792502401 x", unix_seconds), Some("1792502401"));
    assert_eq!(unix_seconds(b"17925024012 x", 0), None);
    assert_eq!(unix_seconds(b"179250240 x", 0), None);
    assert_eq!(logcat(b"10-20 15:20:01.123 x"), Some(18));
    assert_eq!(logcat(b"10-20 15:20 x"), None);
    assert_eq!(glog(b"E1020 15:20:01.123456 x"), Some(21));
    assert_eq!(glog(b"E1020x15:20:01"), None);
}

#[test]
fn syslog_needs_a_month_a_day_and_a_clock() {
    assert_eq!(
        taken("Oct 20 15:20:01 host", syslog),
        Some("Oct 20 15:20:01")
    );
    assert_eq!(
        taken("Oct 20 15:20:01 2026x", syslog),
        Some("Oct 20 15:20:01")
    );
    for text in [
        "Oct 20",
        "Oct 20 15:20",
        "OCT 20 15:20:01",
        "Oct   1 15:20:01",
        "Oct 200 15:20:01",
    ] {
        assert_eq!(taken(text, syslog), None, "{text}");
    }
}
