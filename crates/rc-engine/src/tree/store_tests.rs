use std::sync::Arc;

use rc_vfs::VfsPath;

use super::{TreeLevel, TreeStore};
use crate::tree::TreeChild;

fn level(path: &str, names: &[&str]) -> TreeLevel {
    TreeLevel {
        path: VfsPath::local(path),
        children: names
            .iter()
            .map(|name| TreeChild {
                name: (*name).to_owned(),
                hidden: false,
                link: false,
            })
            .collect::<Arc<[TreeChild]>>(),
        truncated: false,
    }
}

#[test]
fn a_kept_level_is_given_back_until_it_is_marked_stale() {
    let store = TreeStore::default();
    let root = level("/", &["etc", "home"]);
    store.put(&root);
    assert_eq!(store.get(&root.path), Some(root.clone()));
    assert_eq!(store.nodes(), 2);
    store.stale(&root.path);
    assert_eq!(store.get(&root.path), None, "a stale level is read again");
    store.put(&level("/", &["etc"]));
    assert_eq!(
        store.nodes(),
        1,
        "the level read again replaces the old one"
    );
}

#[test]
fn past_the_limit_the_subtree_folded_longest_ago_goes_first() {
    let store = TreeStore::with_limit(6);
    store.put(&level("/", &["a", "b"]));
    store.put(&level("/a", &["a1", "a2"]));
    store.put(&level("/a/a1", &["x"]));
    store.put(&level("/b", &["b1"]));
    store.folded(&VfsPath::local("/a"));
    store.folded(&VfsPath::local("/b"));
    // Three more: /a was folded first, so it goes with /a/a1 below it.
    store.put(&level("/c", &["1", "2", "3"]));
    assert!(!store.holds(&VfsPath::local("/a")));
    assert!(!store.holds(&VfsPath::local("/a/a1")));
    assert!(store.holds(&VfsPath::local("/b")), "folded later, it stays");
    assert!(store.holds(&VfsPath::local("/c")));
    assert_eq!(store.nodes(), 6);
}

#[test]
fn an_unfolded_level_and_those_above_it_are_not_given_up_first() {
    let store = TreeStore::with_limit(4);
    store.put(&level("/", &["a", "b"]));
    store.put(&level("/a", &["a1"]));
    store.folded(&VfsPath::local("/"));
    store.unfolded(&VfsPath::local("/a"));
    store.put(&level("/b", &["b1"]));
    store.put(&level("/z", &["z1"]));
    // Nothing is folded any more: the level asked for longest ago goes, never the one just read.
    assert!(store.holds(&VfsPath::local("/z")));
    assert!(store.nodes() <= 4);
}

#[test]
fn clearing_forgets_every_level() {
    let store = TreeStore::default();
    store.put(&level("/", &["a"]));
    store.clear();
    assert_eq!(store.nodes(), 0);
    assert!(!store.holds(&VfsPath::local("/")));
}
