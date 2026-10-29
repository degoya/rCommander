use chrono::{NaiveDate, NaiveDateTime};

use super::{Fields, NoTimestamp, path_levels, render};
use crate::pattern::parse;
use crate::plan::CounterSettings;

/// A name of 20 characters: `01` at 5, `T` at 9, `060306` at 10 to 15, `P` at 16.
const NAME: &str = "abcd01xyT060306Pqrst";
const FILE_NAME: &str = "abcd01xyT060306Pqrst.mp3";

fn time(hour: u32, minute: u32, second: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2019, 1, 2)
        .and_then(|date| date.and_hms_opt(hour, minute, second))
        .expect("a valid date")
}

/// `mask` for `<NAME>.mp3` in `...\TechPodcast\Download`, the counter at `index` of `count`.
fn rendered_with(
    mask: &str,
    index: u64,
    count: u64,
    modified: Option<NaiveDateTime>,
) -> Result<String, NoTimestamp> {
    let pattern = parse(mask).unwrap_or_else(|error| panic!("{mask}: {error}"));
    let fields = Fields {
        name: NAME,
        extension: "mp3",
        file_name: FILE_NAME,
        parent: "Download",
        grandparent: "TechPodcast",
        index,
        count,
        counter: CounterSettings::default(),
        modified,
    };
    render(&pattern, &fields)
}

fn rendered(mask: &str) -> String {
    rendered_with(mask, 0, 1, Some(time(9, 2, 7)))
        .unwrap_or_else(|missing| panic!("{mask}: {missing:?}"))
}

#[test]
fn ranges_take_the_characters_total_commander_documents() {
    let cases = [
        ("[N]", NAME),
        ("[N1]", "a"),
        ("[N9]", "T"),
        ("[N-5]", "P"),
        ("[N2-5]", "bcd0"),
        ("[N2,5]", "bcd01"),
        ("[N18-]", "rst"),
        ("[N-8,5]", "306Pq"),
        ("[N-8-5]", "306P"),
        ("[N2--5]", "bcd01xyT060306P"),
        ("[N-5-]", "Pqrst"),
        ("[N10,6]", "060306"),
        ("[E]", "mp3"),
        ("[E1-2]", "mp"),
        ("[E-1]", "3"),
        ("[A]", FILE_NAME),
        ("[A-5-]", "t.mp3"),
        ("[P]", "Download"),
        ("[P1-4]", "Down"),
        ("[G]", "TechPodcast"),
        ("[G-7-]", "Podcast"),
        // Outside the text: the part that lies inside, or nothing.
        ("[N30]", ""),
        ("[N18-30]", "rst"),
        ("[N19,5]", "st"),
        ("[N-30-2]", "abcd01xyT060306Pqrs"),
        ("[N5-2]", ""),
    ];
    for (mask, expected) in cases {
        assert_eq!(rendered(mask), expected, "{mask}");
    }
}

#[test]
fn path_levels_drop_drives_prefixes_and_empty_parts() {
    let cases: [(&str, &[&str]); 5] = [
        (r"C:\Users\me", &["Users", "me"]),
        (r"\\?\C:\data", &["data"]),
        (r"\\nas\media\film", &["nas", "media", "film"]),
        ("/home//me/", &["home", "me"]),
        ("/", &[]),
    ];
    for (dir, expected) in cases {
        assert_eq!(path_levels(dir), expected, "{dir}");
    }
}

#[test]
fn the_counter_counts_by_index_with_rule_or_inline_settings() {
    // (mask, index, count, expected)
    let cases = [
        ("[C]", 0, 1, "1"),
        ("[C]", 4, 5, "5"),
        ("[C:3]", 4, 5, "005"),
        ("[C10+5:3]", 0, 3, "010"),
        ("[C10+5:3]", 2, 3, "020"),
        ("[C+2]", 3, 4, "7"),
        ("[C-2]", 0, 1, "-2"),
        ("[C0+-1:2]", 3, 4, "-03"),
        // `:a`: as many digits as the run's last value needs.
        ("[C:a]", 0, 12, "01"),
        ("[C:a]", 0, 9, "1"),
        ("[C10+10:a]", 0, 12, "010"),
    ];
    for (mask, index, count, expected) in cases {
        let text = rendered_with(mask, index, count, None).expect("no date needed");
        assert_eq!(text, expected, "{mask} at {index} of {count}");
    }
}

#[test]
fn date_and_time_placeholders_format_the_modification_time() {
    let cases = [
        ("[Y]", "2019"),
        ("[y]", "19"),
        ("[M]", "01"),
        ("[D]", "02"),
        ("[h]", "09"),
        ("[H]", "09"),
        ("[m]", "02"),
        ("[s]", "07"),
        ("[i]", "am"),
        ("[i1]", "a"),
        ("[YMD]", "20190102"),
        ("[hms]", "090207"),
        ("[T1][Y]-[M]-[D] [h].[m]", "2019-01-02 09.02"),
    ];
    for (mask, expected) in cases {
        assert_eq!(rendered(mask), expected, "{mask}");
    }
    let evening = |mask: &str| rendered_with(mask, 0, 1, Some(time(21, 5, 0)));
    assert_eq!(evening("[h] [H][i] [H][i1]").as_deref(), Ok("21 09pm 09p"));
}

#[test]
fn a_date_without_a_modification_time_is_missing() {
    assert_eq!(rendered_with("[Y]", 0, 1, None), Err(NoTimestamp));
    assert_eq!(rendered_with("[N]", 0, 1, None).as_deref(), Ok(NAME));
}

#[test]
fn case_placeholders_change_everything_after_them() {
    let cases = [
        ("[U][N1-4][n]_[N5-]", "ABCD_01xyT060306Pqrst"),
        ("x[L]AbC [F]hello wORLD", "xabc Hello World"),
        ("[F]a[N1]", "Aa"),
        ("[U][E]", "MP3"),
        ("[L]ABC[n]DEF", "abcDEF"),
        ("[[][U]x[]]", "[X]"),
    ];
    for (mask, expected) in cases {
        assert_eq!(rendered(mask), expected, "{mask}");
    }
}
