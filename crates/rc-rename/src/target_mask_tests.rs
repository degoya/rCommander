use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::*;

fn here(name: &str) -> MaskSource<'_> {
    MaskSource {
        name,
        same_folder: true,
    }
}

fn there(name: &str) -> MaskSource<'_> {
    MaskSource {
        name,
        same_folder: false,
    }
}

/// The five examples of Total Commander's help (`proc_copy.htm`, history 23.05.24 and
/// 29.05.24) give exactly its names.
#[test]
fn the_examples_of_the_help_give_its_names() {
    assert_eq!(apply_target_mask("h?.txt", "test.txt"), "he.txt");
    assert_eq!(apply_target_mask("f*.txt", "test.txt"), "fest.txt");
    assert_eq!(apply_target_mask("F*Rest.*", "test.txt"), "FestRest.txt");
    assert_eq!(
        apply_target_mask("start_**_rest.*", "test.txt"),
        "start_test_rest.txt"
    );
    assert_eq!(
        apply_target_mask("start_*?do*_rest.*", "name.txt"),
        "start_dome_rest.txt"
    );
}

#[test]
fn star_dot_star_and_star_change_nothing() {
    for name in ["test.txt", "archive.tar.gz", "README", ".profile", "a.b.c"] {
        assert_eq!(apply_target_mask("*.*", name), name);
        assert_eq!(apply_target_mask("*", name), name);
    }
}

#[test]
fn star_dot_bak_changes_only_the_extension() {
    assert_eq!(apply_target_mask("*.bak", "test.txt"), "test.bak");
    assert_eq!(
        apply_target_mask("*.bak", "archive.tar.gz"),
        "archive.tar.bak",
        "the last dot splits"
    );
    assert_eq!(apply_target_mask("*.bak", "test"), "test.bak");
    assert_eq!(apply_target_mask("*.bak", ".profile"), ".profile.bak");
    assert_eq!(apply_target_mask("x.*", "test.txt"), "x.txt");
    assert_eq!(
        apply_target_mask("*.", "test.txt"),
        "test",
        "no extension left"
    );
}

#[test]
fn a_question_mark_past_the_end_of_the_name_adds_nothing() {
    assert_eq!(apply_target_mask("??????.*", "ab.txt"), "ab.txt");
    assert_eq!(apply_target_mask("a?????b.*", "xy.txt"), "ayb.txt");
    assert_eq!(apply_target_mask("*.???", "a.md"), "a.md");
    assert_eq!(
        apply_target_mask("[N].*", "a.txt"),
        "[N].txt",
        "no placeholders"
    );
}

#[test]
fn a_mask_longer_than_a_rule_text_is_refused() {
    let long = "x".repeat(MAX_RULE_TEXT + 1);
    assert_eq!(
        check_target_mask(&long).map_err(|error| error.code()),
        Err("rename_input_too_long")
    );
    assert!(check_target_mask(&"x".repeat(MAX_RULE_TEXT)).is_ok());
    let refused = mask_targets(&long, &[there("a.txt")], false, false);
    assert_eq!(
        refused.map_err(|error| error.code()),
        Err("rename_input_too_long")
    );
}

#[test]
fn wildcards_are_star_and_question_mark() {
    assert!(has_wildcards("*.bak"));
    assert!(has_wildcards("a?.txt"));
    assert!(!has_wildcards("plain.txt"));
}

#[test]
fn every_entry_gets_its_name_in_order() {
    let sources = [there("a.txt"), there("b.txt"), there("c.md")];
    let targets = mask_targets("*.bak", &sources, false, false).expect("no conflict");
    assert_eq!(
        targets,
        vec![
            Some("a.bak".to_owned()),
            Some("b.bak".to_owned()),
            Some("c.bak".to_owned())
        ]
    );
}

#[test]
fn two_entries_with_one_new_name_are_refused_and_named() {
    let sources = [there("a.txt"), there("b.txt")];
    let error = mask_targets("x.*", &sources, false, false).expect_err("a collision");
    assert_eq!(
        error,
        RenameError::MaskCollision {
            first: "a.txt".into(),
            second: "b.txt".into(),
            name: "x.txt".into(),
        }
    );
    assert_eq!(error.code(), "mask_collision");
    // `A.bak` and `a.bak` are one name only where the target ignores case.
    let sources = [there("A.txt"), there("a.txt")];
    assert!(mask_targets("*.bak", &sources, false, false).is_ok());
    let error = mask_targets("*.bak", &sources, false, true).expect_err("one name");
    assert_eq!(error.code(), "mask_collision");
}

#[test]
fn a_copy_beside_itself_that_keeps_its_name_is_refused() {
    let error = mask_targets("a?.txt", &[here("ab.txt")], false, false).expect_err("unchanged");
    assert_eq!(
        error,
        RenameError::MaskUnchanged {
            name: "ab.txt".into()
        }
    );
    assert_eq!(error.code(), "mask_unchanged");
    // Into another folder the same name is a plain copy.
    assert_eq!(
        mask_targets("a?.txt", &[there("ab.txt")], false, false).expect("a copy"),
        vec![Some("ab.txt".to_owned())]
    );
}

#[test]
fn a_rename_passes_over_an_unchanged_name_and_refuses_when_nothing_changes() {
    let sources = [here("a.old"), here("b.txt")];
    assert_eq!(
        mask_targets("*.old", &sources, true, false).expect("one rename"),
        vec![None, Some("b.old".to_owned())]
    );
    let error = mask_targets("*.old", &[here("a.old")], true, false).expect_err("nothing");
    assert_eq!(error.code(), "mask_unchanged");
}

#[test]
fn a_rename_onto_the_old_name_of_another_is_a_chain() {
    let sources = [here("a1.txt"), here("b1.txt")];
    let error = mask_targets("b*.txt", &sources, true, false).expect_err("a chain");
    assert_eq!(
        error,
        RenameError::MaskChain {
            entry: "a1.txt".into(),
            name: "b1.txt".into(),
        }
    );
    assert_eq!(error.code(), "mask_chain");
    // A copy onto the name of another meets an existing file: a conflict the job asks about.
    let sources = [here("a.txt"), here("ax.txt")];
    assert_eq!(
        mask_targets("*x.txt", &sources, true, false).map_err(|error| error.code()),
        Err("mask_chain")
    );
    assert!(mask_targets("*x.txt", &sources, false, false).is_ok());
    // A change of case alone is no chain.
    assert_eq!(
        mask_targets("*.TXT", &[here("a.txt")], true, true).expect("a rename"),
        vec![Some("a.TXT".to_owned())]
    );
}

/// 100 000 names worked out and checked for a collision in well under a second, also in a debug
/// build: the check is one hash set, never a comparison of every pair.
#[test]
fn a_hundred_thousand_names_take_less_than_a_second() {
    let names: Vec<String> = (0..100_000).map(|n| format!("file{n:06}.txt")).collect();
    let sources: Vec<MaskSource<'_>> = names.iter().map(|name| there(name)).collect();
    let started = Instant::now();
    let targets = mask_targets("*.bak", &sources, false, true).expect("no collision");
    let elapsed = started.elapsed();
    let unique: HashSet<&Option<String>> = targets.iter().collect();
    assert_eq!(unique.len(), 100_000);
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}
