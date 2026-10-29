//! The status line's numbers: a selection change counts only the names that changed, and comes
//! to what a count of the whole folder says (RC-080-53).

use rc_engine::{
    DirSize, Entry, EntryKind, ListMode, PanelState, PanelView, Selection, SortOrder, VfsPath,
};
use rc_vfs::Attributes;

use super::Summary;

fn entry(name: &str, kind: EntryKind, size: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// A folder with two directories and 40 files of 10 bytes each, nothing selected.
fn folder() -> PanelState {
    let mut entries = vec![
        entry("inner", EntryKind::Dir, 0),
        entry("outer", EntryKind::Dir, 0),
    ];
    entries.extend((0..40).map(|i| entry(&format!("f{i:02}.txt"), EntryKind::File, 10)));
    PanelState {
        path: VfsPath::local("/data/work"),
        entries: entries.into(),
        cursor: 0,
        selection: Selection::default(),
        view: PanelView::Files,
        sort: SortOrder::default(),
        mode: ListMode::Full,
        show_hidden: false,
        remembered: Default::default(),
        filter: Default::default(),
        flat: None,
        archive: None,
        dir_sizes: [("inner".to_owned(), DirSize::Bytes(500))].into(),
        listing: None,
    }
}

/// `old` with `names` selected and the cursor on `cursor`.
fn selecting(old: &PanelState, names: &[&str], cursor: usize) -> PanelState {
    let mut new = old.clone();
    new.selection = names.iter().map(|name| (*name).to_owned()).collect();
    new.cursor = cursor;
    new
}

#[test]
fn a_toggle_counts_what_a_whole_count_says() {
    let old = folder();
    let summary = Summary::of(&old);
    // Insert on the counted folder: the cursor moved on to the next row.
    let one = selecting(&old, &["inner"], 1);
    let changed = summary.reselected(&old, &one).expect("one name changed");
    assert_eq!(changed, Summary::of(&one));
    assert_eq!((changed.selected_dirs, changed.selected_bytes), (1, 500));
    // A file far from either cursor, then one taken off again.
    let two = selecting(&one, &["inner", "f30.txt"], 2);
    let changed = changed.reselected(&one, &two).expect("one more");
    assert_eq!(changed, Summary::of(&two));
    let back = selecting(&two, &["f30.txt"], 2);
    let changed = changed.reselected(&two, &back).expect("one less");
    assert_eq!(changed, Summary::of(&back));
    assert_eq!((changed.selected_files, changed.selected_bytes), (1, 10));
}

#[test]
fn many_changed_names_or_an_unlisted_one_count_again() {
    let old = folder();
    let summary = Summary::of(&old);
    let names: Vec<String> = (0..20).map(|i| format!("f{i:02}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(summary.reselected(&old, &selecting(&old, &names, 0)), None);
    assert_eq!(
        summary.reselected(&old, &selecting(&old, &["gone.txt"], 0)),
        None
    );
}
