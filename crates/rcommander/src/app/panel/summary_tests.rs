//! The status line's numbers: a selection change counts only the names that changed, and comes
//! to what a count of the whole folder says (RC-080-53).

use std::collections::BTreeMap;

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
        accessed: None,
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
        dir_sizes: BTreeMap::from([("inner".to_owned(), DirSize::Bytes(500))]).into(),
        columns: Default::default(),
        listing: None,
        git: None,
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

/// `old` with the counted sizes `sizes`, in a map of its own as the engine hands out a batch.
fn sizing(old: &PanelState, sizes: &[(&str, DirSize)]) -> PanelState {
    let mut new = old.clone();
    new.dir_sizes = sizes
        .iter()
        .map(|(name, size)| ((*name).to_owned(), *size))
        .collect::<BTreeMap<_, _>>()
        .into();
    new
}

/// RC-110-63: a batch of counted folders adds only the sizes that changed and comes to what a
/// count of the whole folder says - a selected folder in the selection's bytes too, a folder
/// still counting or no longer counted as nothing, a name the listing does not show nowhere.
#[test]
fn counted_sizes_add_only_what_changed() {
    let old = selecting(&folder(), &["outer", "f00.txt"], 0);
    let summary = Summary::of(&old);
    let folders = super::listed_folders(&old);
    let counting = sizing(
        &old,
        &[
            ("inner", DirSize::Bytes(500)),
            ("outer", DirSize::Counting),
            ("filtered", DirSize::Counting),
        ],
    );
    let step = summary.resized(
        &old.dir_sizes,
        &counting.dir_sizes,
        &old.selection,
        &folders,
    );
    assert_eq!(step, Summary::of(&counting));
    let counted = sizing(
        &old,
        &[
            ("inner", DirSize::Bytes(700)),
            ("outer", DirSize::Bytes(300)),
            ("filtered", DirSize::Bytes(999)),
        ],
    );
    let step = step.resized(
        &counting.dir_sizes,
        &counted.dir_sizes,
        &old.selection,
        &folders,
    );
    assert_eq!(step, Summary::of(&counted));
    assert_eq!((step.bytes, step.selected_bytes), (1400, 310));
    // A folder that could not be counted leaves the map.
    let failed = sizing(&old, &[("outer", DirSize::Bytes(300))]);
    let step = step.resized(
        &counted.dir_sizes,
        &failed.dir_sizes,
        &old.selection,
        &folders,
    );
    assert_eq!(step, Summary::of(&failed));
}

/// The pane carries its summary through a batch of sizes and a selection change in one new
/// state, and counts the whole folder again for new entries.
#[test]
fn the_pane_adds_counted_sizes_to_its_summary() {
    let old = folder();
    let mut pane = super::super::Pane::new(old.clone());
    let mut new = sizing(
        &old,
        &[
            ("inner", DirSize::Bytes(500)),
            ("outer", DirSize::Bytes(40)),
        ],
    );
    new.selection = ["outer".to_owned()].into();
    pane.show(new.clone(), false);
    assert_eq!(*pane.summary(), Summary::of(&new));
    assert_eq!(pane.summary().selected_bytes, 40);
    let mut relisted = sizing(&new, &[("outer", DirSize::Bytes(41))]);
    relisted.entries = new
        .entries
        .iter()
        .skip(1)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    pane.show(relisted.clone(), false);
    assert_eq!(*pane.summary(), Summary::of(&relisted));
}
