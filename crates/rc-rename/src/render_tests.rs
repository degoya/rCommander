use chrono::{NaiveDate, NaiveDateTime};

use super::{Fields, NoTimestamp, Switches, Times, from_root, path_levels, render};
use crate::clock::{DEFAULT_DATE_FORMAT, DEFAULT_TIME_FORMAT};
use crate::pattern::parse;
use crate::plan::CounterSettings;

/// A name of 20 characters: `01` at 5, `T` at 9, `060306` at 10 to 15, `P` at 16.
const NAME: &str = "abcd01xyT060306Pqrst";
const FILE_NAME: &str = "abcd01xyT060306Pqrst.mp3";
/// `C:\Podcasts\TechPodcast\Download`.
static LEVELS: [&str; 3] = ["Podcasts", "TechPodcast", "Download"];
static FROM_ROOT: [&str; 4] = ["C", "Podcasts", "TechPodcast", "Download"];

fn time(hour: u32, minute: u32, second: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2019, 1, 2)
        .and_then(|date| date.and_hms_opt(hour, minute, second))
        .expect("a valid date")
}

/// `<NAME>.mp3` of 1234 bytes in `C:\Podcasts\TechPodcast\Download`, the counter at `index` of
/// `count`.
fn fields(index: u64, count: u64, modified: Option<NaiveDateTime>) -> Fields<'static> {
    Fields {
        name: NAME,
        extension: "mp3",
        file_name: FILE_NAME,
        levels: &LEVELS,
        from_root: &FROM_ROOT,
        size: Some(1234),
        is_dir: false,
        index,
        count,
        counter: CounterSettings::default(),
        times: Times {
            modified,
            opened: None,
            changed: None,
        },
        clipboard: "",
        date_format: DEFAULT_DATE_FORMAT,
        time_format: DEFAULT_TIME_FORMAT,
    }
}

fn render_fields(mask: &str, fields: &Fields<'_>) -> Result<String, NoTimestamp> {
    let pattern = parse(mask).unwrap_or_else(|error| panic!("{mask}: {error}"));
    render(&pattern, fields, &mut Switches::default())
}

fn rendered_with(
    mask: &str,
    index: u64,
    count: u64,
    modified: Option<NaiveDateTime>,
) -> Result<String, NoTimestamp> {
    render_fields(mask, &fields(index, count, modified))
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

#[test]
fn title_case_in_a_mask_keeps_small_words_inside() {
    let cases = [
        ("[f]the lord OF the rings", "The Lord of the Rings"),
        ("[f]what it is for", "What It Is For"),
        ("[f]a [N1-4] of [n]the end", "A Abcd of the end"),
    ];
    for (mask, expected) in cases {
        assert_eq!(rendered(mask), expected, "{mask}");
    }
}

#[test]
fn size_placeholder() {
    assert_eq!(rendered("[N]_[S]"), format!("{NAME}_1234"));
    let unknown = Fields {
        size: None,
        ..fields(0, 1, None)
    };
    assert_eq!(render_fields("[S]", &unknown).as_deref(), Ok("0"));
    let folder = Fields {
        size: Some(4096),
        is_dir: true,
        ..fields(0, 1, None)
    };
    assert_eq!(render_fields("[S]", &folder).as_deref(), Ok("0"));
}

#[test]
fn ignore_dots_in_folder_names() {
    let folder = Fields {
        name: "release",
        extension: "v2",
        file_name: "release.v2",
        is_dir: true,
        ..fields(0, 1, None)
    };
    let masks = |name: &str, extension: &str, fields: &Fields<'_>| {
        let mut switches = Switches::default();
        let name = render(&parse(name).expect("parses"), fields, &mut switches);
        let extension = render(&parse(extension).expect("parses"), fields, &mut switches);
        (name.expect("no date"), extension.expect("no date"))
    };
    assert_eq!(
        masks("[N]", "[E]", &folder),
        ("release".to_owned(), "v2".to_owned())
    );
    // The switch carries into the extension mask.
    assert_eq!(
        masks("[I][N]", "[E]", &folder),
        ("release.v2".to_owned(), String::new())
    );
    assert_eq!(
        masks("[I1][N][I0]_[N]", "[E]", &folder),
        ("release.v2_release".to_owned(), "v2".to_owned())
    );
    // A file keeps its extension.
    assert_eq!(
        masks("[I][N]", "[E]", &fields(0, 1, None)),
        (NAME.to_owned(), "mp3".to_owned())
    );
}

#[test]
fn letter_counter_wraps_to_three_letters() {
    // (mask, index, expected)
    let cases = [
        ("[Caa+1]", 0, "aa"),
        ("[Caa+1]", 1, "ab"),
        ("[Caa+1]", 25, "az"),
        ("[Caa+1]", 26, "ba"),
        ("[Caa+1]", 675, "zz"),
        ("[Caa+1]", 676, "aaa"),
        ("[Caa+1]", 677, "aab"),
        ("[CAAA]", 0, "AAA"),
        ("[CAAA]", 2, "AAC"),
        ("[Cab+2]", 1, "ad"),
        ("[Cz]", 1, "aa"),
        ("[Cc+-1]", 5, "a"),
    ];
    for (mask, index, expected) in cases {
        let text = rendered_with(mask, index, 1000, None).expect("no date needed");
        assert_eq!(text, expected, "{mask} at {index}");
    }
}

#[test]
fn fractional_counter_moves_every_hundred() {
    // (mask, index, expected)
    let cases = [
        ("[C+1/100]", 0, "1"),
        ("[C+1/100]", 99, "1"),
        ("[C+1/100]", 100, "2"),
        ("[C+1/100]", 250, "3"),
        ("[C0+1/100:2]", 199, "01"),
        ("[C+1/100]\\[N1-4]", 100, "2\\abcd"),
    ];
    for (mask, index, expected) in cases {
        let text = rendered_with(mask, index, 300, None).expect("no date needed");
        assert_eq!(text, expected, "{mask} at {index}");
    }
}

#[test]
fn t2_t3_and_offsets() {
    let at = |day: u32, hour: u32, minute: u32, second: u32| {
        NaiveDate::from_ymd_opt(2024, 5, day)
            .and_then(|date| date.and_hms_opt(hour, minute, second))
    };
    let timed = Fields {
        times: Times {
            modified: Some(time(9, 2, 7)),
            opened: at(6, 10, 0, 0),
            changed: at(6, 10, 30, 15),
        },
        ..fields(0, 1, None)
    };
    let cases = [
        ("[YMD]", "20190102"),
        ("[T2][YMD]_[hms]", "20240506_100000"),
        ("[T3][hms]", "103015"),
        ("[T2][h][T1][h]", "1009"),
        ("[T1+60m][h][m]", "1002"),
        ("[T2-1D][D]", "05"),
        ("[T2+1M][M]", "06"),
        ("[T2-1Y][Y]", "2023"),
        ("[T3+45s][m][s]", "3100"),
        ("[T1-2h][h]", "07"),
    ];
    for (mask, expected) in cases {
        assert_eq!(
            render_fields(mask, &timed).as_deref(),
            Ok(expected),
            "{mask}"
        );
    }
    // Without the dialog's moment there is nothing to read.
    assert_eq!(
        render_fields("[T2][Y]", &fields(0, 1, Some(time(9, 2, 7)))),
        Err(NoTimestamp)
    );
}

#[test]
fn local_date_and_time_replace_separators() {
    let morning = fields(0, 1, Some(time(9, 2, 7)));
    assert_eq!(
        render_fields("[d]_[t]", &morning).as_deref(),
        Ok("2019-01-02_09.02.07")
    );
    let cases = [
        ("dd.MM.yyyy", "h:mm tt", "02.01.2019_9.02 AM"),
        ("M/d/yyyy", "HH:mm", "1-2-2019_09.02"),
        ("d. MMMM yy", "H.mm 'Uhr'", "2. January 19_9.02 Uhr"),
        ("ddd, MMM d", "hh:mm:ss t", "Wed, Jan 2_09.02.07 A"),
    ];
    for (date, time_format, expected) in cases {
        let local = Fields {
            date_format: date,
            time_format,
            ..morning
        };
        assert_eq!(
            render_fields("[d]_[t]", &local).as_deref(),
            Ok(expected),
            "{date} {time_format}"
        );
    }
}

#[test]
fn path_parts_b0_to_b9() {
    let cases = [
        ("[B0]", "Download"),
        ("[B1]", "TechPodcast"),
        ("[B2]", "Podcasts"),
        ("[B3]", ""),
        ("[B9]", ""),
        ("[B+0]", "C"),
        ("[B+1]", "Podcasts"),
        ("[B+3]", "Download"),
        ("[B+4]", ""),
        ("[B0_1-4]", "Down"),
        ("[B+1_-4-]", "asts"),
    ];
    for (mask, expected) in cases {
        assert_eq!(rendered(mask), expected, "{mask}");
    }
    let roots: [(&str, &[&str]); 5] = [
        (r"C:\a\b", &["C", "a", "b"]),
        (r"\\?\C:\data", &["C", "data"]),
        (r"\\nas\media\film", &["nas", "media", "film"]),
        (r"\\?\UNC\nas\share", &["nas", "share"]),
        ("/home/me", &["", "home", "me"]),
    ];
    for (dir, expected) in roots {
        assert_eq!(from_root(dir), expected, "{dir}");
    }
}
