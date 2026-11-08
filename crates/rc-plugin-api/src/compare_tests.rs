use super::{ChangeKind, CompareSummary, Comparison, MAX_CHANGES};
use crate::WIT;

#[test]
fn the_kinds_are_named_as_the_contract_names_them() {
    let names: Vec<&str> = ChangeKind::ALL.iter().map(|kind| kind.as_str()).collect();
    assert!(WIT.contains(&format!("enum change-kind {{ {} }}", names.join(", "))));
}

#[test]
fn the_limit_is_the_one_the_contract_states() {
    assert!(WIT.contains(&format!(
        "// at most {} {:03} changes",
        MAX_CHANGES / 1000,
        MAX_CHANGES % 1000
    )));
}

#[test]
fn the_summary_counts_each_kind() {
    let mut summary = CompareSummary::default();
    for kind in [ChangeKind::Added, ChangeKind::Changed, ChangeKind::Changed] {
        summary.count(kind);
    }
    assert_eq!(summary.of(ChangeKind::Changed), 2);
    assert_eq!(summary.of(ChangeKind::Removed), 0);
    assert_eq!(summary.total(), 3);
    assert!(Comparison::default().is_equal());
}

#[test]
fn a_change_serializes_without_what_it_lacks() {
    let change = super::Change {
        kind: ChangeKind::Changed,
        path: "/a".to_owned(),
        column: None,
        old_value: Some("1".to_owned()),
        new_value: Some("2".to_owned()),
        old_line: None,
        new_line: Some(3),
    };
    let json = serde_json::to_string(&change).expect("serializes");
    assert_eq!(
        json,
        r#"{"kind":"changed","path":"/a","old_value":"1","new_value":"2","new_line":3}"#
    );
}
