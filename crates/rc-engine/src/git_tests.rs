use std::collections::BTreeMap;

use rc_vfs::VfsPath;

use super::{GitMark, GitScope, GitView};

fn view(fold_case: bool, whole: Option<GitMark>) -> GitView {
    GitView {
        folder: VfsPath::local("/repo"),
        branch: None,
        marks: BTreeMap::from([("main.rs".to_owned(), GitMark::Modified)]),
        whole,
        deleted: 0,
        fold_case,
    }
}

#[test]
fn an_entry_finds_its_mark_in_its_case_or_in_any() {
    assert_eq!(view(false, None).mark("main.rs"), Some(GitMark::Modified));
    assert_eq!(view(false, None).mark("Main.rs"), None);
    assert_eq!(view(true, None).mark("Main.rs"), Some(GitMark::Modified));
    assert_eq!(view(false, None).mark("other.rs"), None);
}

#[test]
fn a_folder_named_whole_marks_every_entry() {
    let ignored = view(false, Some(GitMark::Ignored));
    assert_eq!(ignored.mark("anything"), Some(GitMark::Ignored));
}

/// The setting's values; anything else is off - nothing runs on a value not understood.
#[test]
fn the_scope_reads_the_setting_and_defaults_to_off() {
    assert_eq!(GitScope::from_setting("local"), GitScope::Local);
    assert_eq!(
        GitScope::from_setting("local_network"),
        GitScope::LocalNetwork
    );
    assert_eq!(GitScope::from_setting("off"), GitScope::Off);
    assert_eq!(GitScope::from_setting("everywhere"), GitScope::Off);
    assert_eq!(GitScope::default(), GitScope::Off);
}

/// The order of importance a folder's dot follows: conflict, modified, added, renamed,
/// untracked; ignored below all.
#[test]
fn marks_are_ordered_by_importance() {
    assert!(GitMark::Conflict > GitMark::Modified);
    assert!(GitMark::Modified > GitMark::Added);
    assert!(GitMark::Added > GitMark::Renamed);
    assert!(GitMark::Renamed > GitMark::Untracked);
    assert!(GitMark::Untracked > GitMark::Ignored);
}
