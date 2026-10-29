use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use super::{RawEntry, RawKind, Tree, file_time, readonly_mode};
use crate::{EntryKind, archive::UnsafeName};

fn file(name: &str, index: u64) -> RawEntry {
    let mut entry = RawEntry::new(name, RawKind::File, index);
    entry.size = 1;
    entry
}

fn dir(name: &str, index: u64) -> RawEntry {
    RawEntry::new(name, RawKind::Dir, index)
}

fn index_at(tree: &Tree, path: &str) -> Option<u64> {
    tree.find(Path::new(path)).expect(path).index
}

fn children(tree: &Tree, path: &str) -> Vec<String> {
    let node = tree.find(Path::new(path)).expect(path);
    tree.children(node)
        .map(|child| child.entry.name.clone())
        .collect()
}

#[test]
fn missing_parent_directories_are_made_up() {
    let tree = Tree::build(vec![file("a/b/c.txt", 0)]);
    let a = tree.find(Path::new("/a")).expect("a");
    assert!(a.is_dir() && a.info.implied && a.index.is_none());
    assert_eq!(children(&tree, "/"), ["a"]);
    assert_eq!(children(&tree, "a/b"), ["c.txt"]);
    let c = tree.find(Path::new("a/b/c.txt")).expect("c");
    assert_eq!((c.entry.kind.clone(), c.index), (EntryKind::File, Some(0)));
    assert!(!c.info.implied);
}

#[test]
fn a_directory_stated_after_its_contents_keeps_them() {
    let mut stated = dir("d/", 1);
    stated.mode = Some(0o750);
    let tree = Tree::build(vec![file("d/x.txt", 0), stated]);
    let d = tree.find(Path::new("/d")).expect("d");
    assert!(!d.info.implied);
    assert_eq!(d.entry.attributes.mode, Some(0o750));
    assert_eq!(children(&tree, "/d"), ["x.txt"]);
}

#[test]
fn a_safe_entry_is_never_replaced_by_an_unsafe_one() {
    let tree = Tree::build(vec![file("safe.txt", 0), file("../safe.txt", 1)]);
    assert_eq!(index_at(&tree, "/safe.txt"), Some(0));
    let tree = Tree::build(vec![file("../safe.txt", 0), file("safe.txt", 1)]);
    assert_eq!(index_at(&tree, "/safe.txt"), Some(1));
    let node = tree.find(Path::new("/safe.txt")).expect("safe");
    assert_eq!(node.info.unsafe_name, None);
}

#[test]
fn of_two_entries_on_one_path_the_later_wins() {
    let tree = Tree::build(vec![file("a.txt", 0), file("a.txt", 1)]);
    assert_eq!(index_at(&tree, "/a.txt"), Some(1));
    let tree = Tree::build(vec![file("../x", 0), file("/x", 1)]);
    let node = tree.find(Path::new("/x")).expect("x");
    assert_eq!(
        (node.index, node.info.unsafe_name),
        (Some(1), Some(UnsafeName::Absolute))
    );
}

#[test]
fn a_file_in_the_way_drops_what_would_be_below_it() {
    let tree = Tree::build(vec![file("a", 0), file("a/b", 1)]);
    assert_eq!(index_at(&tree, "/a"), Some(0));
    let below = tree.find(Path::new("/a/b"));
    assert_eq!(
        below.map(|_| ()).map_err(|error| error.code()),
        Err("not_a_directory")
    );
}

#[test]
fn a_directory_with_contents_is_not_replaced_by_a_file() {
    let tree = Tree::build(vec![file("a/b", 0), file("a", 1)]);
    assert!(tree.find(Path::new("/a")).expect("a").is_dir());
    assert_eq!(index_at(&tree, "/a/b"), Some(0));
}

#[test]
fn names_with_nothing_left_are_skipped() {
    let tree = Tree::build(vec![dir("./", 0), file("..", 1), dir("/", 2)]);
    assert!(children(&tree, "/").is_empty());
}

#[test]
fn only_paths_inside_the_archive_are_found() {
    let tree = Tree::build(vec![file("a.txt", 0)]);
    for root in ["/", "", "."] {
        assert!(tree.find(Path::new(root)).expect(root).is_dir(), "{root}");
    }
    let code = |path: &str| {
        tree.find(Path::new(path))
            .map(|_| ())
            .map_err(|error| error.code())
    };
    assert_eq!(code("../a.txt"), Err("invalid_path"));
    assert_eq!(code("/missing"), Err("not_found"));
    assert_eq!(code("/a.txt"), Ok(()));
}

#[test]
fn file_times_and_modes_convert() {
    const EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    assert_eq!(file_time(0), None);
    assert_eq!(file_time(EPOCH_TICKS), Some(SystemTime::UNIX_EPOCH));
    assert_eq!(
        file_time(EPOCH_TICKS + 1_715_949_296 * 10_000_000 + 5),
        Some(SystemTime::UNIX_EPOCH + Duration::new(1_715_949_296, 500))
    );
    assert_eq!(
        file_time(EPOCH_TICKS - 10_000_000),
        SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(1))
    );
    assert!(readonly_mode(Some(0o444)));
    assert!(!readonly_mode(Some(0o644)));
    assert!(!readonly_mode(None));
}
