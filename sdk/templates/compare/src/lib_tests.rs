use super::{DUPLICATE_KEY, compare};
use crate::types::ChangeKind;

#[test]
fn reordered_lines_are_no_change() {
    let found = compare(b"a=1\nb=2\n", b"b = 2\n# moved\na=1\n").expect("text");
    assert!(found.changes.is_empty());
    assert!(found.notices.is_empty());
}

#[test]
fn each_key_reports_what_became_of_it() {
    let found = compare(b"a=1\nb=2\nc=3\n", b"a=1\nb=20\nd=4\n").expect("text");
    let kinds: Vec<(ChangeKind, &str)> = found
        .changes
        .iter()
        .map(|change| (change.kind, change.path.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (ChangeKind::Changed, "b"),
            (ChangeKind::Removed, "c"),
            (ChangeKind::Added, "d"),
        ]
    );
    let changed = &found.changes[0];
    assert_eq!(changed.old_value.as_deref(), Some("2"));
    assert_eq!(changed.new_value.as_deref(), Some("20"));
    assert_eq!((changed.old_line, changed.new_line), (Some(2), Some(2)));
    assert_eq!(found.count(ChangeKind::Changed), 1);
}

#[test]
fn a_key_found_twice_is_a_notice() {
    let found = compare(b"a=1\na=2\n", b"a=2\n").expect("text");
    assert!(found.changes.is_empty());
    assert_eq!(found.notices.len(), 1);
    assert_eq!(found.notices[0].code, DUPLICATE_KEY);
}

#[test]
fn bytes_that_are_no_text_are_bad_data() {
    assert!(compare(&[0xff, 0xfe, 0x00], b"a=1").is_err());
}
