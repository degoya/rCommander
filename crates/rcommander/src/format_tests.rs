use std::time::{Duration, SystemTime};

use rc_engine::EntryKind;

use super::{count, date_in, date_pattern, parse_size, percent, rights, size};

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;

/// What the find-files dialog's size fields took before the parsers were one, and fractions now.
#[test]
fn sizes_read_as_total_commander_writes_them() {
    assert_eq!(parse_size("512"), Some(512));
    assert_eq!(parse_size("0"), Some(0));
    assert_eq!(parse_size("10k"), Some(10 * KIB));
    assert_eq!(parse_size("2 MB"), Some(2 * MIB));
    assert_eq!(parse_size("1GiB"), Some(GIB));
    assert_eq!(parse_size("1.5m"), Some(MIB + MIB / 2));
    assert_eq!(parse_size("k"), None);
    assert_eq!(parse_size("3 parsecs"), None);
    assert_eq!(parse_size("99999999999999999999g"), None);
}

/// What the split dialog's part size took: decimals with a point or a comma, any case.
#[test]
fn sizes_read_with_a_fraction_and_a_binary_unit() {
    assert_eq!(parse_size("4294967295"), Some(4_294_967_295));
    assert_eq!(parse_size("700 MB"), Some(700 * MIB));
    assert_eq!(parse_size("700m"), Some(700 * MIB));
    assert_eq!(parse_size("1.5G"), Some(GIB + GIB / 2));
    assert_eq!(parse_size("1,5 GiB"), Some(GIB + GIB / 2));
    assert_eq!(parse_size(" 100 KiB "), Some(100 * KIB));
    assert_eq!(parse_size("12 b"), Some(12));
    assert_eq!(parse_size(".5k"), Some(512));
    assert_eq!(
        parse_size("0.0001"),
        Some(0),
        "a fraction of a byte rounds down"
    );
    for text in [
        "",
        "MB",
        "1 TB",
        "1.",
        "1.2.3 M",
        "-5",
        "abc",
        "99999999999 G",
    ] {
        assert_eq!(parse_size(text), None, "{text:?}");
    }
}

#[test]
fn rights_read_as_ls_prints_them() {
    assert_eq!(rights(&EntryKind::Dir, Some(0o755)), "drwxr-xr-x");
    assert_eq!(rights(&EntryKind::File, Some(0o640)), "-rw-r-----");
    assert_eq!(rights(&EntryKind::File, Some(0o4755)), "-rwsr-xr-x");
    assert_eq!(rights(&EntryKind::Dir, Some(0o1777)), "drwxrwxrwt");
    assert_eq!(rights(&EntryKind::File, Some(0o2644)), "-rw-r-Sr--");
    let link = EntryKind::Symlink {
        target: "x".into(),
        target_kind: None,
    };
    assert_eq!(rights(&link, Some(0o777)), "lrwxrwxrwx");
    assert_eq!(rights(&EntryKind::File, None), "");
}

#[test]
fn counts_are_grouped_by_three() {
    assert_eq!(count(0), "0");
    assert_eq!(count(999), "999");
    assert_eq!(count(1_000), "1\u{202f}000");
    assert_eq!(count(1_234_567), "1\u{202f}234\u{202f}567");
}

#[test]
fn sizes_read_in_binary_units() {
    assert_eq!(size(0), "0 B");
    assert_eq!(size(1_023), "1023 B");
    assert_eq!(size(1_024), "1.0 KiB");
    assert_eq!(size(1_536), "1.5 KiB");
    assert_eq!(size(20 * 1024 * 1024), "20 MiB");
    assert_eq!(size(3 * 1024 * 1024 * 1024 / 2), "1.5 GiB");
    assert_eq!(size(u64::MAX), "16777216 TiB");
}

#[test]
fn a_date_has_minute_precision_and_none_is_empty() {
    let iso = "%Y-%m-%d %H:%M";
    assert_eq!(date_in(None, iso), "");
    let text = date_in(
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        iso,
    );
    assert_eq!(text.len(), "2023-11-14 22:13".len());
    assert!(text.starts_with("2023-11-1"), "{text}");
}

#[test]
fn the_locale_orders_day_month_and_year() {
    assert_eq!(date_pattern(Some("de_DE.UTF-8")), "%d.%m.%Y %H:%M");
    assert_eq!(date_pattern(Some("de-AT")), "%d.%m.%Y %H:%M");
    assert_eq!(date_pattern(Some("en-US")), "%m/%d/%Y %I:%M %p");
    assert_eq!(date_pattern(Some("en_GB.UTF-8")), "%d/%m/%Y %H:%M");
    assert_eq!(date_pattern(Some("fr-FR")), "%d/%m/%Y %H:%M");
    assert_eq!(date_pattern(Some("ja-JP")), "%Y/%m/%d %H:%M");
    for iso in [Some("C.UTF-8"), Some("sv-SE"), Some("en"), None] {
        assert_eq!(date_pattern(iso), "%Y-%m-%d %H:%M", "{iso:?}");
    }
}

#[test]
fn a_share_reads_as_a_whole_percent() {
    assert_eq!(percent(0.0), "0 %");
    assert_eq!(percent(0.424), "42 %");
    assert_eq!(percent(1.0), "100 %");
}
