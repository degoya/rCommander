use std::path::{Path, PathBuf};

use rc_vfs::{Attributes, Entry, EntryKind};

use super::{Item, Subtrees, subtree};

fn item(rel: &str, kind: EntryKind, size: u64) -> Item {
    Item {
        path: Path::new("/src").join(rel),
        rel: PathBuf::from(rel),
        entry: Entry {
            name: rel.rsplit('/').next().unwrap_or(rel).to_owned(),
            kind,
            size,
            modified: None,
            created: None,
            attributes: Attributes::default(),
            hidden: false,
            owner: None,
        },
    }
}

#[test]
fn a_subtree_counts_its_files_not_its_directories() {
    let items = [
        item("a", EntryKind::Dir, 0),
        item("a/one", EntryKind::File, 3),
        item("a/b", EntryKind::Dir, 0),
        item("a/b/two", EntryKind::File, 4),
        item("ab", EntryKind::File, 100),
    ];
    assert_eq!(subtree(&items, Path::new("a")), (2, 7));
    assert_eq!(subtree(&items, Path::new("a/b")), (1, 4));
    assert_eq!(
        subtree(&items, Path::new("ab")),
        (1, 100),
        "a file is its own subtree"
    );
}

/// RC-080-45: by index range a subtree counts what the search by path counts, and giving it up
/// skips exactly its items.
#[test]
fn subtrees_by_range_count_and_skip_as_the_search_by_path() {
    let items = [
        item("a", EntryKind::Dir, 0),
        item("a/one", EntryKind::File, 3),
        item("a/b", EntryKind::Dir, 0),
        item("a/b/two", EntryKind::File, 4),
        item("a/c", EntryKind::File, 5),
        item("ab", EntryKind::File, 100),
    ];
    let mut subtrees = Subtrees::new(&items);
    assert!(!subtrees.skipped(0), "nothing given up");
    for (at, item) in items.iter().enumerate() {
        assert_eq!(subtrees.tally(at), subtree(&items, &item.rel), "{at}");
    }
    assert_eq!(subtrees.by_range, Some(true));
    assert_eq!(subtrees.give_up(2), (1, 4));
    let skipped: Vec<bool> = (0..items.len()).map(|at| subtrees.skipped(at)).collect();
    assert_eq!(skipped, [false, false, true, true, false, false]);
}

/// Two sources of one name (a flat view's `x/f` and `y/f`) share their paths: the plan is
/// searched by path, as it always was.
#[test]
fn sources_of_one_name_are_searched_by_path() {
    let items = [
        item("f", EntryKind::Dir, 0),
        item("f/one", EntryKind::File, 3),
        item("g", EntryKind::File, 1),
        item("f", EntryKind::Dir, 0),
        item("f/two", EntryKind::File, 4),
    ];
    let mut subtrees = Subtrees::new(&items);
    assert_eq!(subtrees.tally(0), (2, 7));
    assert_eq!(subtrees.by_range, Some(false));
    assert_eq!(subtrees.give_up(0), (2, 7));
    let skipped: Vec<bool> = (0..items.len()).map(|at| subtrees.skipped(at)).collect();
    assert_eq!(skipped, [true, true, false, true, true]);
}
