//! Hostile answers of a compare plugin (RC-100-41, third acceptance criterion): huge values are
//! cut, a flood of changes is refused, counts that contradict the list are refused, notices in
//! another's namespace dropped.

use rc_plugin_api::compare::{ChangeKind, MAX_CHANGES, MAX_NOTICES};

use super::{MAX_CHANGE_VALUE_CHARS, MAX_PATH_CHARS, comparison};
use crate::bindings::compare_world::exports::rcommander::plugin::compare as wit;
use crate::codes;

fn change(kind: wit::ChangeKind, path: &str) -> wit::Change {
    wit::Change {
        kind,
        path: path.to_owned(),
        column: None,
        old_value: Some("1".to_owned()),
        new_value: Some("2".to_owned()),
        old_line: Some(1),
        new_line: Some(0),
    }
}

fn summary(changed: u32, truncated: bool) -> wit::CompareSummary {
    wit::CompareSummary {
        added: 0,
        removed: 0,
        changed,
        moved: 0,
        truncated,
    }
}

fn answer(changes: Vec<wit::Change>, summary: wit::CompareSummary) -> wit::Comparison {
    wit::Comparison {
        summary,
        changes,
        notices: Vec::new(),
    }
}

#[test]
fn a_plain_answer_passes_with_line_zero_as_no_line() {
    let checked = comparison(answer(
        vec![change(wit::ChangeKind::Changed, "/a")],
        summary(1, false),
    ))
    .expect("valid");
    assert_eq!(checked.changes.len(), 1);
    assert_eq!(checked.changes[0].kind, ChangeKind::Changed);
    assert_eq!(checked.changes[0].old_line, Some(1));
    assert_eq!(checked.changes[0].new_line, None);
    assert!(!checked.summary.truncated);
}

#[test]
fn huge_values_and_paths_are_cut() {
    let mut huge = change(wit::ChangeKind::Changed, &"p".repeat(MAX_PATH_CHARS * 50));
    huge.old_value = Some("x".repeat(4 * 1024 * 1024));
    huge.new_value = Some("line\nbreak\u{202e}".repeat(10_000));
    let checked = comparison(answer(vec![huge], summary(1, false))).expect("valid");
    let change = &checked.changes[0];
    assert!(change.path.chars().count() <= MAX_PATH_CHARS + 1);
    let old = change.old_value.as_deref().expect("kept");
    assert!(old.chars().count() <= MAX_CHANGE_VALUE_CHARS + 1);
    assert!(old.ends_with('\u{2026}'));
    let new = change.new_value.as_deref().expect("kept");
    assert!(!new.contains('\n') && !new.contains('\u{202e}'), "{new}");
}

#[test]
fn more_changes_than_the_limit_are_refused() {
    let flood: Vec<wit::Change> = (0..=MAX_CHANGES)
        .map(|index| change(wit::ChangeKind::Added, &format!("/{index}")))
        .collect();
    let mut counted = summary(0, true);
    counted.added = u32::MAX;
    let refused = comparison(answer(flood, counted)).expect_err("too many");
    assert_eq!(refused.code(), Some(codes::PLUGIN_COMPARE_TOO_LARGE));
}

#[test]
fn the_limit_itself_is_taken_and_a_larger_count_marks_it_partial() {
    let full: Vec<wit::Change> = (0..MAX_CHANGES)
        .map(|index| change(wit::ChangeKind::Changed, &format!("/{index}")))
        .collect();
    let counted = summary(5_000_000, false);
    let checked = comparison(answer(full, counted)).expect("at the limit");
    assert_eq!(checked.changes.len(), MAX_CHANGES);
    assert_eq!(checked.summary.changed, 5_000_000);
    assert!(checked.summary.truncated);
}

#[test]
fn a_count_below_the_list_is_bad_output() {
    let refused = comparison(answer(
        vec![
            change(wit::ChangeKind::Changed, "/a"),
            change(wit::ChangeKind::Changed, "/b"),
        ],
        summary(1, false),
    ))
    .expect_err("contradicts itself");
    assert_eq!(refused.code(), Some(codes::PLUGIN_BAD_OUTPUT));
}

#[test]
fn notices_keep_the_plugins_own_codes_only_and_a_bounded_number() {
    let mut raw = answer(Vec::new(), summary(0, false));
    raw.notices.push(wit::Notice {
        code: "plugin_crashed".to_owned(),
        params: Vec::new(),
    });
    for _ in 0..(MAX_NOTICES + 20) {
        raw.notices.push(wit::Notice {
            code: "data-compare.duplicate_key".to_owned(),
            params: vec![
                ("key".to_owned(), "k\u{202e}".repeat(1000)),
                ("bad key!".to_owned(), "dropped".to_owned()),
            ],
        });
    }
    let checked = comparison(raw).expect("valid");
    assert_eq!(checked.notices.len(), MAX_NOTICES);
    assert!(
        checked
            .notices
            .iter()
            .all(|notice| notice.code == "data-compare.duplicate_key")
    );
    let params = &checked.notices[0].params;
    assert_eq!(params.len(), 1);
    assert!(!params[0].1.contains('\u{202e}'));
    assert!(checked.is_equal());
}
