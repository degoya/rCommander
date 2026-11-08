use super::*;

#[test]
fn hex_pairs_with_colons() {
    assert_eq!(hex(&[0x0d, 0x12, 0xec]), "0D:12:EC");
    assert_eq!(hex(&[]), "");
}

#[test]
fn dates_in_utc() {
    assert_eq!(date(0), "1970-01-01 00:00:00 UTC");
    assert_eq!(date(1_798_761_600), "2027-01-01 00:00:00 UTC");
    assert_eq!(date(951_782_400 + 3_661), "2000-02-29 01:01:01 UTC");
    assert_eq!(date(-1), "1969-12-31 23:59:59 UTC");
    // The extremes of an i64 do not overflow.
    assert!(!date(i64::MAX).is_empty());
    assert!(!date(i64::MIN).is_empty());
}

#[test]
fn days_round_towards_the_past() {
    assert_eq!(days_between(0, DAY), 1);
    assert_eq!(days_between(0, DAY - 1), 0);
    assert_eq!(days_between(DAY, 0), -1);
    assert_eq!(days_between(i64::MIN, i64::MAX), i64::MAX / DAY);
}

#[test]
fn text_is_cleaned_and_capped() {
    assert_eq!(clean("a\u{1b}[31mb\nc"), "a\u{fffd}[31mb\u{fffd}c");
    let long = "x".repeat(MAX_VALUE_CHARS + 10);
    assert_eq!(clean(&long).chars().count(), MAX_VALUE_CHARS + 3);
    assert_eq!(clean_bytes(&[0x61, 0xff]), "a\u{fffd}");
}
