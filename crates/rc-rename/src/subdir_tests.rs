use std::path::Path;

use super::{SUBFOLDER_SEPARATOR, check_path, nested_path, normalized};
use crate::names::{FsRules, InvalidName};
use crate::plan::{Change, Conflict, PlanInput, RenameRule, RenameSource, plan};
use crate::schedule::schedule;

const DIR: &str = r"C:\photos";

fn sources(names: &[&str]) -> Vec<RenameSource> {
    names
        .iter()
        .map(|name| RenameSource {
            name: (*name).to_owned(),
            modified: None,
        })
        .collect()
}

fn rule(name_mask: &str) -> RenameRule {
    RenameRule {
        name_mask: name_mask.to_owned(),
        ..RenameRule::default()
    }
}

fn planned(names: &[&str], existing: &[&str], mask: &str) -> crate::plan::RenamePlan {
    let sources = sources(names);
    let existing: Vec<String> = existing.iter().map(|name| (*name).to_owned()).collect();
    let input = PlanInput {
        dir: DIR,
        sources: &sources,
        existing: &existing,
        fs: FsRules::WINDOWS,
    };
    plan(&input, &rule(mask)).expect("the rule plans")
}

#[test]
fn a_backslash_is_spelled_with_the_subfolder_separator() {
    assert_eq!(SUBFOLDER_SEPARATOR, '/');
    assert_eq!(normalized(r"a\b\c.txt"), "a/b/c.txt");
    assert_eq!(normalized("a/b"), "a/b");
}

#[test]
fn a_path_stays_below_the_directory() {
    let cases = [
        ("sub/a.txt", None),
        ("2024/03/a.txt", None),
        ("a.txt", None),
        ("../x", Some(InvalidName::ParentDir)),
        ("sub/../x", Some(InvalidName::ParentDir)),
        ("C:/x", Some(InvalidName::DrivePath)),
        ("//server/x", Some(InvalidName::UncPath)),
        ("//?/C:/x", Some(InvalidName::UncPath)),
        ("/x", Some(InvalidName::AbsolutePath)),
        ("a//b", Some(InvalidName::EmptyPart)),
        ("a/", Some(InvalidName::EmptyPart)),
        ("sub/./x", Some(InvalidName::DotName)),
        ("sub/CON", Some(InvalidName::ReservedName)),
        ("su?b/x", Some(InvalidName::ReservedChar { ch: '?' })),
        ("sub /x", Some(InvalidName::TrailingDotOrSpace)),
    ];
    for (path, expected) in cases {
        assert_eq!(check_path(path, DIR, &FsRules::WINDOWS), expected, "{path}");
    }
}

#[test]
fn a_nested_path_counts_against_the_longest_path() {
    let rules = FsRules {
        max_path: 20,
        ..FsRules::UNIX
    };
    assert_eq!(check_path("ab/cd", "/0123456789", &rules), None);
    assert_eq!(
        check_path("abcd/efgh", "/0123456789", &rules),
        Some(InvalidName::PathTooLong)
    );
}

#[test]
fn a_nested_path_is_joined_part_by_part() {
    let dir = Path::new("/d");
    assert_eq!(nested_path(dir, "sub/x.txt"), dir.join("sub").join("x.txt"));
    assert_eq!(nested_path(dir, "x.txt"), dir.join("x.txt"));
    // A backslash is part of a name on Linux, never a separator here.
    assert_eq!(nested_path(dir, r"a\b"), dir.join(r"a\b"));
}

#[test]
fn a_backslash_in_the_mask_moves_into_a_subfolder() {
    let plan = planned(&["a.txt", "b.txt"], &["a.txt", "b.txt"], r"sub\[N]");
    let lines: Vec<(&str, &str, Change, Option<Conflict>)> = plan
        .items
        .iter()
        .map(|item| {
            (
                item.old.as_str(),
                item.new.as_str(),
                item.change,
                item.conflict,
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("a.txt", "sub/a.txt", Change::Rename, None),
            ("b.txt", "sub/b.txt", Change::Rename, None),
        ]
    );
    let steps = schedule(&plan, "t").expect("schedules").steps;
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].to, "sub/a.txt");
}

#[test]
fn paths_out_of_the_directory_are_conflicts_with_their_reason_and_never_scheduled() {
    let cases = [
        (r"..\[N]", InvalidName::ParentDir),
        (r"C:\[N]", InvalidName::DrivePath),
        (r"\\server\[N]", InvalidName::UncPath),
        (r"a\\[N]", InvalidName::EmptyPart),
        (r"\[N]", InvalidName::AbsolutePath),
    ];
    for (mask, reason) in cases {
        let plan = planned(&["a.txt"], &["a.txt"], mask);
        assert_eq!(
            plan.items[0].conflict,
            Some(Conflict::InvalidName { reason }),
            "{mask}"
        );
        assert_eq!(
            plan.items[0].conflict.map(|conflict| conflict.code()),
            Some(reason.code()),
            "{mask}"
        );
        assert!(schedule(&plan, "t").is_err(), "{mask} must never run");
    }
}

#[test]
fn a_name_taken_in_the_subfolder_is_a_conflict() {
    let plan = planned(&["a.txt"], &["a.txt", "sub/a.txt"], r"sub\[N]");
    assert_eq!(plan.items[0].conflict, Some(Conflict::TargetExists));
    let plan = planned(&["a.txt", "a.TXT"], &["a.txt", "a.TXT"], r"sub\[N]");
    assert!(matches!(
        plan.items[0].conflict,
        Some(Conflict::Duplicate { other: 1 })
    ));
}
