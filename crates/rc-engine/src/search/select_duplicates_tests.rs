use std::time::{Duration, SystemTime};

use super::{
    DatePick, DuplicateRule, DuplicateSelection, Keep, Listed, SizePick, apply, duplicate_folders,
    folder_matches,
};
use crate::state::Selection;

fn at(seconds: u64) -> Option<SystemTime> {
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

/// Two groups: 0 with three copies of different ages, 1 with two of the same age and size.
fn result() -> Vec<Listed<'static>> {
    vec![
        Listed {
            name: "a/photo.jpg",
            group: 0,
            modified: at(200),
            size: 10,
        },
        Listed {
            name: "b/photo.jpg",
            group: 0,
            modified: at(100),
            size: 10,
        },
        Listed {
            name: "b/old/photo.jpg",
            group: 0,
            modified: at(300),
            size: 10,
        },
        Listed {
            name: "a/x.txt",
            group: 1,
            modified: at(50),
            size: 5,
        },
        Listed {
            name: "c/x.txt",
            group: 1,
            modified: at(50),
            size: 5,
        },
    ]
}

fn selected(rule: DuplicateRule, leave_one: bool, keep: Vec<Keep>) -> Vec<String> {
    let mut selection = Selection::default();
    let spec = DuplicateSelection {
        rule,
        leave_one,
        keep,
    };
    apply(&spec, &result(), &mut selection);
    selection.iter().cloned().collect()
}

#[test]
fn just_the_oldest_per_group() {
    let names = selected(
        DuplicateRule::Date {
            pick: DatePick::Oldest,
        },
        false,
        super::default_keep(),
    );
    assert_eq!(
        names,
        ["a/x.txt", "b/photo.jpg"],
        "equal ages: the first in the list"
    );
}

#[test]
fn all_but_the_newest() {
    let names = selected(
        DuplicateRule::Date {
            pick: DatePick::AllOlder,
        },
        false,
        super::default_keep(),
    );
    assert_eq!(
        names,
        ["a/photo.jpg", "b/photo.jpg"],
        "group 1 has no older file"
    );
    let newer = selected(
        DuplicateRule::Date {
            pick: DatePick::AllNewer,
        },
        false,
        super::default_keep(),
    );
    assert_eq!(newer, ["a/photo.jpg", "b/old/photo.jpg"]);
}

#[test]
fn by_size_takes_nothing_from_equal_sizes() {
    let names = selected(
        DuplicateRule::Size {
            pick: SizePick::AllSmaller,
        },
        false,
        super::default_keep(),
    );
    assert!(names.is_empty());
    let largest = selected(
        DuplicateRule::Size {
            pick: SizePick::Largest,
        },
        false,
        super::default_keep(),
    );
    assert_eq!(largest, ["a/photo.jpg", "a/x.txt"]);
}

#[test]
fn keep_one_falls_back_to_size_then_position() {
    let every = DuplicateRule::Mask {
        mask: "*".to_owned(),
        remove: false,
    };
    // Group 0: the newest decides (b/old/photo.jpg stays). Group 1: same date, same size - the
    // first in the list stays.
    let names = selected(
        every.clone(),
        true,
        vec![Keep::Newest, Keep::Largest, Keep::First],
    );
    assert_eq!(names, ["a/photo.jpg", "b/photo.jpg", "c/x.txt"]);
    let last = selected(
        every.clone(),
        true,
        vec![Keep::Newest, Keep::Largest, Keep::Last],
    );
    assert_eq!(last, ["a/photo.jpg", "a/x.txt", "b/photo.jpg"]);
    let unkept = selected(every, false, Vec::new());
    assert_eq!(
        unkept.len(),
        5,
        "without the switch whole groups are selected"
    );
}

#[test]
fn a_mask_selects_and_removes_by_file_name() {
    let mut selection = Selection::default();
    let mut spec = DuplicateSelection {
        rule: DuplicateRule::Mask {
            mask: "*.jpg".to_owned(),
            remove: false,
        },
        leave_one: false,
        keep: super::default_keep(),
    };
    apply(&spec, &result(), &mut selection);
    assert_eq!(selection.len(), 3);
    spec.rule = DuplicateRule::Mask {
        mask: "photo.*".to_owned(),
        remove: true,
    };
    apply(&spec, &result(), &mut selection);
    assert!(selection.is_empty());
}

#[test]
fn select_by_folder_with_subfolders() {
    let folders = |subfolders| DuplicateRule::Folders {
        folders: vec!["b".to_owned()],
        pattern: None,
        parent_only: false,
        subfolders,
        remove: false,
    };
    assert_eq!(selected(folders(false), false, Vec::new()), ["b/photo.jpg"]);
    assert_eq!(
        selected(folders(true), false, Vec::new()),
        ["b/old/photo.jpg", "b/photo.jpg"]
    );
}

#[test]
fn a_folder_pattern_applies_to_every_name_or_only_the_parent() {
    let by_pattern = |parent_only| DuplicateRule::Folders {
        folders: Vec::new(),
        pattern: Some("b".to_owned()),
        parent_only,
        subfolders: false,
        remove: false,
    };
    assert_eq!(
        selected(by_pattern(false), false, Vec::new()),
        ["b/old/photo.jpg", "b/photo.jpg"],
        "b/old has b on its way"
    );
    assert_eq!(
        selected(by_pattern(true), false, Vec::new()),
        ["b/photo.jpg"]
    );
    assert!(folder_matches("b/old", "o*", true));
    assert!(
        !folder_matches("", "*", false),
        "the panel's folder has no name"
    );
}

#[test]
fn the_folders_of_a_result_once_each() {
    let names = result();
    assert_eq!(
        duplicate_folders(names.iter().map(|file| file.name)),
        ["a", "b", "b/old", "c"]
    );
}
