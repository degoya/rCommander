//! The plan with the context and the name lists of RC-100-96.

use super::tests::{file, lines, planned_with, rule};
use super::{Change, Conflict, MAX_NAMES, RenameContext, RenameRule, RenameSource};
use crate::case::CaseMode;
use crate::error::RenameError;
use crate::names::FsRules;

fn clipboard(text: Option<&str>) -> RenameContext {
    RenameContext {
        clipboard: text.map(str::to_owned),
        ..RenameContext::default()
    }
}

fn news(rule: &RenameRule, context: &RenameContext, sources: &[RenameSource]) -> Vec<String> {
    planned_with(sources, &[], FsRules::UNIX, rule, context)
        .expect("plans")
        .items
        .into_iter()
        .map(|item| item.new)
        .collect()
}

#[test]
fn clipboard_placeholder_errors_when_empty_and_plus_does_not() {
    let sources = [file("a.txt")];
    for text in [None, Some(""), Some("\r\n")] {
        let error = planned_with(
            &sources,
            &[],
            FsRules::UNIX,
            &rule("[N]_[X]", "[E]"),
            &clipboard(text),
        )
        .expect_err("[X] needs text");
        assert_eq!(error, RenameError::ClipboardEmpty, "{text:?}");
        assert_eq!(error.code(), "rename_clipboard_empty");
        assert_eq!(
            news(&rule("[N]_[X+]", "[E]"), &clipboard(text), &sources),
            ["a_.txt"],
            "{text:?}"
        );
    }
    // A mask without [X] does not care.
    assert_eq!(
        news(&rule("[N]", "[E]"), &clipboard(None), &sources),
        ["a.txt"]
    );
    assert_eq!(
        news(
            &rule("[X]", "[E]"),
            &clipboard(Some("Holiday 2024\r\n")),
            &sources
        ),
        ["Holiday 2024.txt"]
    );
}

#[test]
fn clipboard_text_loses_forbidden_characters() {
    let sources = [file("a.txt")];
    let context = clipboard(Some("why? a*b:c|d\"e<f>g"));
    assert_eq!(
        news(&rule("[X]", "[E]"), &context, &sources),
        ["why_ a_b_c_d_e_f_g.txt"]
    );
    let long = clipboard(Some(&"x".repeat(super::MAX_RULE_TEXT + 1)));
    let error = planned_with(&sources, &[], FsRules::UNIX, &rule("[X]", "[E]"), &long)
        .expect_err("too long");
    assert_eq!(error.code(), "rename_input_too_long");
}

#[test]
fn names_from_a_list_rename_in_order() {
    let sources = [file("c.txt"), file("a.txt"), file("b.txt")];
    let listed = RenameRule {
        names: Some(vec![
            "one.md".to_owned(),
            "two".to_owned(),
            "three.txt".to_owned(),
            "x".to_owned(),
        ]),
        // The masks are not read: an invalid one does not stop the list.
        ..rule("[N", "[E]")
    };
    assert_eq!(
        news(&listed, &RenameContext::default(), &sources),
        ["one.md", "two", "three.txt"]
    );
    // Case and search still apply.
    let upper = RenameRule {
        case: CaseMode::Upper,
        ..listed
    };
    assert_eq!(
        news(&upper, &RenameContext::default(), &sources),
        ["ONE.MD", "TWO", "THREE.TXT"]
    );
}

#[test]
fn too_few_names_is_a_conflict() {
    let sources = [file("a.txt"), file("b.txt"), file("c.txt")];
    let listed = RenameRule {
        names: Some(vec!["x.txt".to_owned()]),
        ..RenameRule::default()
    };
    let plan = planned_with(
        &sources,
        &[],
        FsRules::UNIX,
        &listed,
        &RenameContext::default(),
    )
    .expect("plans");
    assert_eq!(
        lines(&plan),
        [
            ("a.txt", "x.txt", Change::Rename, None),
            (
                "b.txt",
                "b.txt",
                Change::Unchanged,
                Some(Conflict::NamesMissing)
            ),
            (
                "c.txt",
                "c.txt",
                Change::Unchanged,
                Some(Conflict::NamesMissing)
            ),
        ]
    );
    assert_eq!(Conflict::NamesMissing.code(), "rename_names_missing");
    // A listed name is held to the naming rules as any new name.
    let bad = RenameRule {
        names: Some(vec!["a:b".to_owned()]),
        ..RenameRule::default()
    };
    let plan = planned_with(
        &sources[..1],
        &[],
        FsRules::UNIX,
        &bad,
        &RenameContext::default(),
    )
    .expect("plans");
    assert!(matches!(
        plan.items[0].conflict,
        Some(Conflict::InvalidName { .. })
    ));
}

#[test]
fn a_name_list_is_bounded() {
    let too_many = RenameRule {
        names: Some(vec!["x".to_owned(); MAX_NAMES + 1]),
        ..RenameRule::default()
    };
    let error = too_many.check().expect_err("too many");
    assert_eq!(error.code(), "rename_names_too_large");
    let too_big = RenameRule {
        names: Some(vec!["x".repeat(super::MAX_NAMES_BYTES + 1)]),
        ..RenameRule::default()
    };
    assert_eq!(
        too_big.check().map_err(|error| error.code()),
        Err("rename_names_too_large")
    );
}

#[test]
fn the_title_case_setting_covers_the_whole_name() {
    let sources = [file("the lord of the rings.mkv")];
    let rule = RenameRule {
        case: CaseMode::Title,
        ..RenameRule::default()
    };
    assert_eq!(
        news(&rule, &RenameContext::default(), &sources),
        ["The Lord of the Rings.Mkv"]
    );
}

#[test]
fn a_folder_keeps_its_dots_after_i() {
    let folder = RenameSource {
        is_dir: true,
        ..file("release.v2")
    };
    let sources = [folder, file("notes.v2")];
    assert_eq!(
        news(
            &rule("[I][N]_old", "[E]"),
            &RenameContext::default(),
            &sources
        ),
        ["release.v2_old", "notes_old.v2"]
    );
}

#[test]
fn an_overlong_format_is_refused() {
    let context = RenameContext {
        date_format: Some("d".repeat(crate::clock::MAX_FORMAT_TEXT + 1)),
        ..RenameContext::default()
    };
    let error = planned_with(
        &[file("a.txt")],
        &[],
        FsRules::UNIX,
        &rule("[d]", "[E]"),
        &context,
    )
    .expect_err("too long");
    assert_eq!(error.code(), "rename_input_too_long");
}
