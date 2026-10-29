use chrono::DateTime;

use super::{UndoRecord, applied_count};
use crate::names::FsRules;
use crate::schedule::{RenameStep, undo_steps};

fn step(from: &str, to: &str) -> RenameStep {
    RenameStep {
        from: from.to_owned(),
        to: to.to_owned(),
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_record_survives_json_and_undoes_its_steps() {
    let record = UndoRecord {
        tag: "7".to_owned(),
        dir: r"C:\photos".to_owned(),
        started: DateTime::from_timestamp(1_700_000_000, 0).expect("a valid time"),
        steps: vec![
            RenameStep {
                from: "a".to_owned(),
                to: ".rc-rename-7-1".to_owned(),
            },
            RenameStep {
                from: "b".to_owned(),
                to: "a".to_owned(),
            },
        ],
    };
    let text = serde_json::to_string(&record).expect("serialises");
    let back: UndoRecord = serde_json::from_str(&text).expect("parses");
    assert_eq!(back, record);
    assert_eq!(undo_steps(&back.steps)[0].from, "a");
}

/// A swap `a <-> b`: `a -> t`, `b -> a`, `t -> b`.
fn swap() -> Vec<RenameStep> {
    vec![step("a", "t"), step("b", "a"), step("t", "b")]
}

#[test]
fn the_marks_count_when_the_listing_agrees() {
    let fs = FsRules::UNIX;
    assert_eq!(applied_count(&swap(), 0, &names(&["a", "b"]), &fs), 0);
    assert_eq!(applied_count(&swap(), 1, &names(&["t", "b"]), &fs), 1);
    assert_eq!(applied_count(&swap(), 2, &names(&["t", "a"]), &fs), 2);
    // Done: the names are the start's again, and only the marks tell 3 from 0.
    assert_eq!(applied_count(&swap(), 3, &names(&["a", "b"]), &fs), 3);
}

#[test]
fn a_step_carried_out_before_its_mark_counts() {
    let fs = FsRules::UNIX;
    assert_eq!(applied_count(&swap(), 0, &names(&["t", "b"]), &fs), 1);
    assert_eq!(applied_count(&swap(), 1, &names(&["t", "a"]), &fs), 2);
    assert_eq!(applied_count(&swap(), 2, &names(&["a", "b"]), &fs), 3);
}

#[test]
fn a_step_reversed_before_its_mark_was_cleared_does_not_count() {
    let fs = FsRules::UNIX;
    // An undo reversed `t -> b` and stopped before clearing its mark.
    assert_eq!(applied_count(&swap(), 3, &names(&["a", "t"]), &fs), 2);
    assert_eq!(applied_count(&swap(), 1, &names(&["a", "b"]), &fs), 0);
}

#[test]
fn a_case_only_change_is_read_through_its_temporary_name() {
    let fs = FsRules::WINDOWS;
    let steps = vec![
        step("readme.TXT", ".rc-rename-7-1"),
        step(".rc-rename-7-1", "README.txt"),
    ];
    assert_eq!(applied_count(&steps, 1, &names(&["README.txt"]), &fs), 2);
    assert_eq!(
        applied_count(&steps, 1, &names(&[".rc-rename-7-1"]), &fs),
        1
    );
    assert_eq!(
        applied_count(&steps, 0, &names(&[".rc-rename-7-1"]), &fs),
        1
    );
    assert_eq!(
        applied_count(&steps, 2, &names(&[".rc-rename-7-1"]), &fs),
        1
    );
    assert_eq!(applied_count(&steps, 0, &names(&["readme.TXT"]), &fs), 0);
}

#[test]
fn marks_past_the_end_are_capped() {
    assert_eq!(
        applied_count(&swap(), 9, &names(&["a", "b"]), &FsRules::UNIX),
        3
    );
}
