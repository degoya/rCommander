use std::sync::Arc;

use rc_vfs::{Entry, EntryKind};

use super::{Selection, by_name};

fn entry(name: &str) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::File,
        size: 0,
        modified: None,
        created: None,
        attributes: rc_vfs::Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn names(names: &[&str]) -> Selection {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_copy_shares_the_names_until_one_side_changes() {
    let mut selection = names(&["a", "b"]);
    let copy = selection.clone();
    assert!(
        Arc::ptr_eq(&selection.0, &copy.0),
        "a copy is no copy of the set"
    );
    assert!(selection.insert("c".to_owned()));
    assert!(!Arc::ptr_eq(&selection.0, &copy.0));
    assert_eq!(selection, ["a", "b", "c"]);
    assert_eq!(copy, ["a", "b"], "the copy keeps what it had");
}

#[test]
fn a_change_that_changes_nothing_copies_nothing() {
    let mut selection = names(&["a"]);
    let copy = selection.clone();
    assert!(!selection.insert("a".to_owned()));
    assert!(!selection.remove("ghost"));
    assert!(Arc::ptr_eq(&selection.0, &copy.0));
    let mut empty = Selection::default();
    let shared = empty.clone();
    empty.clear();
    assert!(Arc::ptr_eq(&empty.0, &shared.0));
    assert!(selection.remove("a") && selection.is_empty());
    selection.clear();
    assert_eq!(copy, ["a"]);
}

#[test]
fn it_reads_as_the_sorted_set_and_serializes_as_a_list() {
    let selection = names(&["b", "a", "b"]);
    assert_eq!(selection.len(), 2);
    assert!(selection.contains("a"));
    assert_eq!(selection.first().map(String::as_str), Some("a"));
    let json = serde_json::to_value(&selection).expect("serializes");
    assert_eq!(json, serde_json::json!(["a", "b"]));
    let back: Selection = serde_json::from_value(json).expect("deserializes");
    assert_eq!(back, selection);
    let owned: Vec<String> = selection.clone().into_iter().collect();
    assert_eq!(owned, ["a", "b"]);
    assert_eq!(format!("{selection:?}"), r#"{"a", "b"}"#);
}

#[test]
fn still_listed_keeps_the_shared_set_while_every_name_is_listed() {
    let entries = [entry("a"), entry("b"), entry("c")];
    let listed = by_name(&entries);
    let selection = names(&["a", "c"]);
    let kept = selection.still_listed(&listed);
    assert!(Arc::ptr_eq(&kept.0, &selection.0));
    let fewer = names(&["a", "gone"]).still_listed(&listed);
    assert_eq!(fewer, ["a"]);
}

#[test]
fn by_name_answers_the_first_entry_of_a_name() {
    let entries = [entry("a"), entry("b"), entry("a")];
    let index = by_name(&entries);
    assert_eq!(index.get("a"), Some(&0));
    assert_eq!(index.get("b"), Some(&1));
    assert_eq!(index.get("z"), None);
}
