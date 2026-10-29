use chrono::NaiveDate;

use super::{
    Change, Conflict, CounterSettings, MAX_RULE_TEXT, PlanInput, RenamePlan, RenameRule,
    RenameSource, plan, split_name,
};
use crate::case::CaseMode;
use crate::error::{MaskField, PatternErrorKind, RenameError};
use crate::names::{FsRules, InvalidName};
use crate::replace::SearchReplace;

fn file(name: &str) -> RenameSource {
    RenameSource {
        name: name.to_owned(),
        modified: NaiveDate::from_ymd_opt(2024, 3, 9).and_then(|date| date.and_hms_opt(14, 5, 0)),
    }
}

fn rule(name_mask: &str, extension_mask: &str) -> RenameRule {
    RenameRule {
        name_mask: name_mask.to_owned(),
        extension_mask: extension_mask.to_owned(),
        ..RenameRule::default()
    }
}

fn planned(
    sources: &[RenameSource],
    existing: &[&str],
    fs: FsRules,
    rule: &RenameRule,
) -> Result<RenamePlan, RenameError> {
    let existing: Vec<String> = existing.iter().map(|name| (*name).to_owned()).collect();
    let input = PlanInput {
        dir: r"C:\photos",
        sources,
        existing: &existing,
        fs,
    };
    plan(&input, rule)
}

/// `(old, new, change, conflict)` of every item.
fn lines(plan: &RenamePlan) -> Vec<(&str, &str, Change, Option<Conflict>)> {
    plan.items
        .iter()
        .map(|item| {
            (
                item.old.as_str(),
                item.new.as_str(),
                item.change,
                item.conflict,
            )
        })
        .collect()
}

#[test]
fn names_split_at_the_last_dot_but_not_at_the_edges() {
    let cases = [
        ("photo.jpg", ("photo", "jpg")),
        ("archive.tar.gz", ("archive.tar", "gz")),
        (".bashrc", (".bashrc", "")),
        ("notes.", ("notes.", "")),
        ("README", ("README", "")),
        // A folder splits the same way, as in Total Commander.
        ("release.v2", ("release", "v2")),
    ];
    for (name, expected) in cases {
        assert_eq!(split_name(name), expected, "{name}");
    }
}

#[test]
fn the_default_rule_changes_nothing() {
    let sources = [file("a.txt"), file(".env"), file("notes."), file("b")];
    let plan = planned(&sources, &[], FsRules::UNIX, &RenameRule::default()).expect("plans");
    assert!(
        plan.items
            .iter()
            .all(|item| item.change == Change::Unchanged && item.old == item.new)
    );
    assert!(!plan.has_conflicts());
}

#[test]
fn masks_case_and_search_apply_in_that_order() {
    let sources = [file("Holiday Beach.JPG"), file("holiday city.jpeg")];
    let rule = RenameRule {
        case: CaseMode::WordsUpper,
        search: Some(SearchReplace {
            find: "Holiday ".to_owned(),
            replace: "2024 ".to_owned(),
            case_sensitive: true,
            ..SearchReplace::default()
        }),
        counter: CounterSettings {
            start: 7,
            step: 1,
            width: 2,
        },
        ..rule("[N] [C] [Y]-[M]-[D]", "[E]")
    };
    let plan = planned(&sources, &[], FsRules::WINDOWS, &rule).expect("plans");
    let news: Vec<&str> = plan.items.iter().map(|item| item.new.as_str()).collect();
    // The case runs before the search, so `holiday` became `Holiday` and was replaced too.
    assert_eq!(
        news,
        [
            "2024 Beach 07 2024-03-09.Jpg",
            "2024 City 08 2024-03-09.Jpeg"
        ]
    );
}

#[test]
fn one_case_setting_covers_the_whole_name() {
    let sources = [file("hello wORLD.TXT"), file("noext")];
    let cases = [
        (CaseMode::FirstUpper, ["Hello world.txt", "Noext"]),
        (CaseMode::WordsUpper, ["Hello World.Txt", "Noext"]),
        (CaseMode::Upper, ["HELLO WORLD.TXT", "NOEXT"]),
        (CaseMode::Lower, ["hello world.txt", "noext"]),
    ];
    for (case, expected) in cases {
        let rule = RenameRule {
            case,
            ..RenameRule::default()
        };
        let plan = planned(&sources, &[], FsRules::UNIX, &rule).expect("plans");
        let news: Vec<&str> = plan.items.iter().map(|item| item.new.as_str()).collect();
        assert_eq!(news, expected, "{case:?}");
    }
}

#[test]
fn the_search_can_include_the_extension() {
    let sources = [file("a.jpeg")];
    let search = |include_extension| SearchReplace {
        find: r"\.jpeg$".to_owned(),
        replace: ".jpg".to_owned(),
        regex: true,
        include_extension,
        ..SearchReplace::default()
    };
    let with = RenameRule {
        search: Some(search(true)),
        ..RenameRule::default()
    };
    let without = RenameRule {
        search: Some(search(false)),
        ..RenameRule::default()
    };
    let new = |rule: &RenameRule| {
        planned(&sources, &[], FsRules::UNIX, rule)
            .expect("plans")
            .items[0]
            .new
            .clone()
    };
    assert_eq!(new(&with), "a.jpg");
    assert_eq!(new(&without), "a.jpeg");
}

#[test]
fn an_empty_extension_mask_leaves_no_dot() {
    let plan = planned(&[file("a.txt")], &[], FsRules::UNIX, &rule("[N]", "")).expect("plans");
    assert_eq!(plan.items[0].new, "a");
}

#[test]
fn conflict_two_entries_get_the_same_new_name() {
    let sources = [file("a.txt"), file("b.txt"), file("c.txt")];
    let rule = rule("same", "[E]");
    let plan = planned(&sources, &[], FsRules::UNIX, &rule).expect("plans");
    assert_eq!(
        lines(&plan),
        [
            (
                "a.txt",
                "same.txt",
                Change::Rename,
                Some(Conflict::Duplicate { other: 1 })
            ),
            (
                "b.txt",
                "same.txt",
                Change::Rename,
                Some(Conflict::Duplicate { other: 0 })
            ),
            (
                "c.txt",
                "same.txt",
                Change::Rename,
                Some(Conflict::Duplicate { other: 0 })
            ),
        ]
    );
    assert_eq!(plan.conflict_count(), 3);
}

#[test]
fn conflict_duplicates_ignore_case_where_the_file_system_does() {
    // `x.txt` and `x.TXT` are one name on NTFS and two on ext4.
    let sources = [file("a.txt"), file("b.TXT")];
    let rule = rule("x", "[E]");
    let windows = planned(&sources, &[], FsRules::WINDOWS, &rule).expect("plans");
    assert_eq!(
        lines(&windows),
        [
            (
                "a.txt",
                "x.txt",
                Change::Rename,
                Some(Conflict::Duplicate { other: 1 })
            ),
            (
                "b.TXT",
                "x.TXT",
                Change::Rename,
                Some(Conflict::Duplicate { other: 0 })
            ),
        ]
    );
    let unix = planned(&sources, &[], FsRules::UNIX, &rule).expect("plans");
    assert!(!unix.has_conflicts(), "{:?}", lines(&unix));
}

#[test]
fn conflict_the_new_name_is_taken_by_an_entry_that_stays() {
    let sources = [file("a.txt")];
    let rule = rule("b", "[E]");
    let plan = planned(&sources, &["a.txt", "B.TXT"], FsRules::WINDOWS, &rule).expect("plans");
    assert_eq!(
        lines(&plan),
        [(
            "a.txt",
            "b.txt",
            Change::Rename,
            Some(Conflict::TargetExists)
        )]
    );
    // On a case-sensitive file system `B.TXT` is another name.
    let plan = planned(&sources, &["a.txt", "B.TXT"], FsRules::UNIX, &rule).expect("plans");
    assert!(!plan.has_conflicts());
}

#[test]
fn a_name_that_is_renamed_away_is_free_and_one_that_stays_is_not() {
    // A swap: the counter counts down, `1.txt -> 2.txt` and `2.txt -> 1.txt`.
    let swap = [file("1.txt"), file("2.txt")];
    let plan = planned(&swap, &[], FsRules::UNIX, &rule("[C2+-1]", "[E]")).expect("plans");
    assert_eq!(
        lines(&plan),
        [
            ("1.txt", "2.txt", Change::Rename, None),
            ("2.txt", "1.txt", Change::Rename, None),
        ]
    );

    // `b.txt` keeps its name (the mask renders it unchanged), so `a.txt -> b.txt` collides.
    let sources = [file("a.txt"), file("b.txt")];
    let rule = RenameRule {
        search: Some(SearchReplace {
            find: "a".to_owned(),
            replace: "b".to_owned(),
            ..SearchReplace::default()
        }),
        ..RenameRule::default()
    };
    let plan = planned(&sources, &[], FsRules::UNIX, &rule).expect("plans");
    assert_eq!(
        lines(&plan),
        [
            (
                "a.txt",
                "b.txt",
                Change::Rename,
                Some(Conflict::TargetExists)
            ),
            ("b.txt", "b.txt", Change::Unchanged, None),
        ]
    );
}

#[test]
fn conflict_names_the_file_system_refuses() {
    let cases = [
        ("[N]?", InvalidName::ReservedChar { ch: '?' }),
        ("CON", InvalidName::ReservedName),
        ("[N].", InvalidName::TrailingDotOrSpace),
        ("", InvalidName::Empty),
        (r"..\[N]", InvalidName::ParentDir),
        ("[N]:x", InvalidName::ReservedChar { ch: ':' }),
        ("..", InvalidName::DotName),
    ];
    for (mask, reason) in cases {
        let plan = planned(&[file("a.txt")], &[], FsRules::WINDOWS, &rule(mask, ""))
            .unwrap_or_else(|error| panic!("{mask}: {error}"));
        assert_eq!(
            plan.items[0].conflict,
            Some(Conflict::InvalidName { reason }),
            "{mask}"
        );
        assert_eq!(
            plan.items[0].conflict.map(|conflict| conflict.code()),
            Some(reason.code())
        );
    }
}

#[test]
fn a_case_only_change_is_flagged_not_a_conflict_where_case_is_ignored() {
    let sources = [file("readme.TXT")];
    let rule = rule("[U][N]", "[L][E]");
    let windows = planned(&sources, &["readme.TXT"], FsRules::WINDOWS, &rule).expect("plans");
    assert_eq!(
        lines(&windows),
        [("readme.TXT", "README.txt", Change::CaseOnly, None)]
    );
    let unix = planned(&sources, &["readme.TXT"], FsRules::UNIX, &rule).expect("plans");
    assert_eq!(
        lines(&unix),
        [("readme.TXT", "README.txt", Change::Rename, None)]
    );
}

#[test]
fn a_date_mask_without_a_modification_time_is_a_conflict_of_that_entry() {
    let undated = RenameSource {
        modified: None,
        ..file("b.txt")
    };
    let plan = planned(
        &[file("a.txt"), undated],
        &[],
        FsRules::UNIX,
        &rule("[N]_[Y]", "[E]"),
    )
    .expect("plans");
    assert_eq!(
        lines(&plan),
        [
            ("a.txt", "a_2024.txt", Change::Rename, None),
            (
                "b.txt",
                "b.txt",
                Change::Unchanged,
                Some(Conflict::NoTimestamp)
            ),
        ]
    );
}

#[test]
fn rule_errors_stop_the_plan() {
    let sources = [file("a.txt")];
    let error = planned(&sources, &[], FsRules::UNIX, &rule("[N", "[E]")).expect_err("bad mask");
    assert!(matches!(
        &error,
        RenameError::Mask {
            field: MaskField::Name,
            error,
        } if error.kind == PatternErrorKind::Unclosed && error.position == 0
    ));
    let error = planned(&sources, &[], FsRules::UNIX, &rule("[N]", "[x]")).expect_err("bad mask");
    assert!(matches!(
        &error,
        RenameError::Mask {
            field: MaskField::Extension,
            ..
        }
    ));
}

#[test]
fn rule_texts_are_bounded() {
    let sources = [file("a.txt")];
    let long = "x".repeat(MAX_RULE_TEXT + 1);
    let fits = "x".repeat(MAX_RULE_TEXT);
    let error = planned(&sources, &[], FsRules::UNIX, &rule(&long, "[E]")).expect_err("too long");
    assert_eq!(error.code(), "rename_input_too_long");
    assert!(planned(&sources, &[], FsRules::UNIX, &rule(&fits, "[E]")).is_ok());

    let replace = RenameRule {
        search: Some(SearchReplace {
            find: "a".to_owned(),
            replace: long,
            ..SearchReplace::default()
        }),
        ..RenameRule::default()
    };
    let error = planned(&sources, &[], FsRules::UNIX, &replace).expect_err("too long");
    assert!(
        matches!(
            error,
            RenameError::TooLong {
                field: "replacement",
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_rule_reads_from_json_with_defaults() {
    let rule: RenameRule = serde_json::from_str(
        r#"{"name_mask": "[N]_[C]", "counter": {"width": 3}, "case": "upper"}"#,
    )
    .expect("parses");
    assert_eq!(rule.extension_mask, "[E]");
    assert_eq!(
        rule.counter,
        CounterSettings {
            start: 1,
            step: 1,
            width: 3
        }
    );
    let plan = planned(&[file("a.txt")], &[], FsRules::UNIX, &rule).expect("plans");
    assert_eq!(plan.items[0].new, "A_001.TXT");
}
