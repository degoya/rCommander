use std::time::{Duration, SystemTime};

use rc_engine::EntryKind;

use super::{
    attributes, count, count_in, date, date_in, date_pattern, day, day_pattern, from_local,
    instant, parse_size, percent, rights, scaled, size,
};
use crate::locale::Lang;

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

/// The tests read English, so `count` groups with a comma.
#[test]
fn counts_are_grouped_by_three() {
    assert_eq!(count(0), "0");
    assert_eq!(count(999), "999");
    assert_eq!(count(1_000), "1,000");
    assert_eq!(count(1_234_567), "1,234,567");
}

/// Every language groups with its own separator (RC-110-83): English a comma, German and
/// Spanish a point, French a narrow no-break space.
#[test]
fn counts_are_grouped_as_each_language_writes_them() {
    for (lang, million, small) in [
        (Lang::En, "1,234,567", "999"),
        (Lang::De, "1.234.567", "999"),
        (Lang::Fr, "1\u{202f}234\u{202f}567", "999"),
        (Lang::Es, "1.234.567", "999"),
    ] {
        assert_eq!(count_in(lang, 1_234_567), million, "{lang:?}");
        assert_eq!(count_in(lang, 999), small, "{lang:?}");
        assert_eq!(count_in(lang, 0), "0", "{lang:?}");
    }
}

/// Every language writes a size's decimal with its own mark (RC-110-83); whole numbers and
/// bytes stay as they are.
#[test]
fn sizes_take_the_decimal_mark_of_each_language() {
    for (lang, mark) in [
        (Lang::En, "1.5"),
        (Lang::De, "1,5"),
        (Lang::Fr, "1,5"),
        (Lang::Es, "1,5"),
    ] {
        assert_eq!(scaled(lang, 1_536), (1, mark.to_owned()), "{lang:?}");
        assert_eq!(scaled(lang, 3 * GIB / 2), (3, mark.to_owned()), "{lang:?}");
        assert_eq!(scaled(lang, 20 * MIB), (2, "20".to_owned()), "{lang:?}");
        assert_eq!(scaled(lang, 1_023), (0, "1023".to_owned()), "{lang:?}");
    }
    assert_eq!(scaled(Lang::De, u64::MAX), (4, "16777216".to_owned()));
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

/// Every pattern the panels, the properties and the dialogs write gives the text chrono gave
/// (RC-110-36): a local time read back and written in each.
#[test]
fn a_local_time_reads_back_in_every_pattern() {
    let local = jiff::civil::date(2024, 5, 17).at(14, 4, 5, 0);
    let time = from_local(local).and_then(|time| SystemTime::try_from(time).ok());
    let cases = [
        ("%Y-%m-%d %H:%M:%S", "2024-05-17 14:04:05"),
        ("%d.%m.%Y %H:%M", "17.05.2024 14:04"),
        ("%m/%d/%Y %I:%M %p", "05/17/2024 02:04 PM"),
        ("%d/%m/%Y %H:%M", "17/05/2024 14:04"),
        ("%d-%m-%Y %H:%M", "17-05-2024 14:04"),
        ("%Y/%m/%d %H:%M", "2024/05/17 14:04"),
        ("%Y%m%d-%H%M%S", "20240517-140405"),
        ("%H:%M", "14:04"),
    ];
    for (pattern, text) in cases {
        assert_eq!(date_in(time, pattern), text, "{pattern}");
    }
}

/// An instant reads as a file's date does, and its day as the date without the time: the
/// update page and the certificate dialog (RC-110-83).
#[test]
fn an_instant_and_its_day_read_as_a_file_date() {
    let at = jiff::Timestamp::from_second(1_700_000_000).expect("in range");
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(instant(at), date(Some(time)));
    assert!(
        instant(at).starts_with(&day(at)),
        "{} {}",
        instant(at),
        day(at)
    );
    assert!(day(at).len() < instant(at).len());
    assert_eq!(day_pattern("%d.%m.%Y %H:%M"), "%d.%m.%Y");
    assert_eq!(day_pattern("%m/%d/%Y %I:%M %p"), "%m/%d/%Y");
    assert_eq!(day_pattern("%H:%M"), "%H:%M");
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

/// A column set's attributes column (RC-100-90): the Windows flags as Total Commander writes them,
/// else the rights.
#[test]
fn attributes_read_as_flags_on_windows_and_as_rights_elsewhere() {
    let entry = |windows, mode, readonly| rc_vfs::Entry {
        name: "a".into(),
        kind: EntryKind::File,
        size: 0,
        modified: None,
        created: None,
        accessed: None,
        attributes: rc_vfs::Attributes {
            mode,
            readonly,
            windows,
        },
        hidden: false,
        owner: None,
    };
    let flags = rc_vfs::WindowsFlags {
        hidden: true,
        system: false,
        archive: true,
    };
    assert_eq!(attributes(&entry(Some(flags), None, true)), "rah-");
    assert_eq!(
        attributes(&entry(Some(rc_vfs::WindowsFlags::default()), None, false)),
        "----"
    );
    assert_eq!(attributes(&entry(None, Some(0o644), false)), "-rw-r--r--");
    assert_eq!(attributes(&entry(None, None, false)), "");
}
