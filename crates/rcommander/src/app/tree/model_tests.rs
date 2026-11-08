use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rc_engine::{
    VfsPath,
    tree::{TreeChild, TreeLevel},
};

use super::{Change, DirTree, Sign, Step};

fn level(path: &str, names: &[&str]) -> TreeLevel {
    TreeLevel {
        path: VfsPath::local(path),
        children: names
            .iter()
            .map(|name| TreeChild {
                name: (*name).to_owned(),
                hidden: false,
                link: name.starts_with('@'),
            })
            .collect::<Arc<[TreeChild]>>(),
        truncated: false,
    }
}

fn at(path: &str) -> VfsPath {
    VfsPath::local(path)
}

/// A tree on `/` with `/a/b` shown: `/` holds `a` and `y`, `/a` holds `b` and `x`, `/a/b` none.
fn revealed() -> DirTree {
    let mut tree = DirTree::new(at("/"), "/".into(), false);
    assert_eq!(tree.reveal(&at("/a/b")), [at("/")]);
    assert_eq!(
        tree.arrived(&at("/"), Ok(level("/", &["a", "y"]))),
        [at("/a")]
    );
    assert_eq!(
        tree.arrived(&at("/a"), Ok(level("/a", &["b", "x"]))),
        [at("/a/b")]
    );
    let none: Vec<VfsPath> = Vec::new();
    assert_eq!(tree.arrived(&at("/a/b"), Ok(level("/a/b", &[]))), none);
    tree
}

fn names(tree: &DirTree) -> Vec<String> {
    tree.visible()
        .iter()
        .filter_map(|row| tree.shown(*row).map(|shown| shown.name.to_owned()))
        .collect()
}

#[test]
fn showing_a_folder_reads_the_levels_on_its_way_one_after_the_other() {
    let tree = revealed();
    assert_eq!(names(&tree), ["/", "a", "b", "x", "y"]);
    assert_eq!(tree.cursor_path(), Some(&at("/a/b")));
    assert!(!tree.revealing());
}

#[test]
fn right_unfolds_and_left_folds_or_goes_up() {
    let mut tree = revealed();
    tree.step(Step::Down);
    assert_eq!(
        tree.step(Step::Open),
        Change::Unfolded {
            path: at("/a/x"),
            load: true
        }
    );
    let row = tree.visible()[3];
    assert_eq!(tree.shown(row).map(|shown| shown.sign), Some(Sign::Loading));
    tree.arrived(&at("/a/x"), Ok(level("/a/x", &[])));
    assert_eq!(tree.shown(row).map(|shown| shown.sign), Some(Sign::Leaf));
    assert_eq!(tree.step(Step::Close), Change::Folded(at("/a/x")));
    assert_eq!(tree.step(Step::Close), Change::Cursor);
    assert_eq!(tree.cursor_path(), Some(&at("/a")));
    assert_eq!(tree.step(Step::Close), Change::Folded(at("/a")));
    assert_eq!(names(&tree), ["/", "a", "y"]);
    // Unfolded again: read before, nothing to read.
    assert_eq!(
        tree.step(Step::Open),
        Change::Unfolded {
            path: at("/a"),
            load: false
        }
    );
}

#[test]
fn a_failure_stays_at_its_folder_and_right_asks_again() {
    let mut tree = revealed();
    tree.step(Step::Down);
    assert_eq!(tree.cursor_path(), Some(&at("/a/x")));
    tree.step(Step::Open);
    tree.arrived(&at("/a/x"), Err("access_denied"));
    let row = tree.visible()[tree.cursor - tree.top];
    let shown = tree.shown(row).expect("a row");
    assert_eq!(
        (shown.sign, shown.error),
        (Sign::Failed, Some("access_denied"))
    );
    assert!(matches!(
        tree.step(Step::Open),
        Change::Unfolded { load: true, .. }
    ));
}

#[test]
fn an_answer_nobody_waits_for_is_dropped() {
    let mut tree = revealed();
    tree.arrived(&at("/y"), Ok(level("/y", &["z"])));
    assert_eq!(names(&tree), ["/", "a", "b", "x", "y"]);
    tree.arrived(&at("/elsewhere"), Ok(level("/elsewhere", &["z"])));
    assert_eq!(tree.folders(), 5);
}

#[test]
fn a_level_read_again_keeps_what_is_still_there() {
    let mut tree = revealed();
    assert_eq!(tree.reload(&at("/")), Some(at("/")));
    assert_eq!(tree.reload(&at("/y")), None, "never read");
    tree.arrived(&at("/"), Ok(level("/", &["a", "new"])));
    assert_eq!(names(&tree), ["/", "a", "b", "x", "new"]);
    assert_eq!(tree.cursor_path(), Some(&at("/a/b")), "the cursor stays");
}

#[test]
fn links_are_listed_but_never_unfolded() {
    let mut tree = DirTree::new(at("/"), "/".into(), false);
    tree.reveal(&at("/"));
    tree.arrived(&at("/"), Ok(level("/", &["@link"])));
    tree.step(Step::Down);
    assert_eq!(tree.step(Step::Open), Change::Nothing);
    let row = tree.visible()[1];
    assert_eq!(tree.shown(row).map(|shown| shown.sign), Some(Sign::Leaf));
}

#[test]
fn letters_jump_to_the_next_folder_read_with_that_start() {
    let mut tree = revealed();
    tree.step(Step::Close);
    tree.step(Step::Close);
    tree.step(Step::Close);
    tree.step(Step::Home);
    let now = Instant::now();
    assert!(tree.jump("x", now), "a folded folder read counts");
    assert_eq!(tree.cursor_path(), Some(&at("/a/x")));
    assert!(tree.jump("y", now + Duration::from_secs(2)));
    assert_eq!(tree.cursor_path(), Some(&at("/y")));
    assert!(!tree.jump("q", now + Duration::from_secs(4)));
    assert_eq!(tree.cursor_path(), Some(&at("/y")));
}

#[test]
fn the_cursor_keys_move_within_the_rows() {
    let mut tree = revealed();
    tree.set_height(2);
    assert_eq!(tree.step(Step::Home), Change::Cursor);
    assert_eq!(tree.step(Step::Up), Change::Nothing);
    tree.step(Step::End);
    assert_eq!(tree.cursor_path(), Some(&at("/y")));
    assert_eq!(tree.visible().len(), 2);
    tree.step(Step::PageUp);
    assert_eq!(tree.cursor, 3);
    assert_eq!(tree.click(0), Change::Cursor);
    assert_eq!(tree.cursor_path(), Some(&at("/")));
}

#[test]
fn f2_reads_the_folder_under_the_cursor_again() {
    let mut tree = revealed();
    assert_eq!(tree.reread(), Some(at("/a/b")));
    assert_eq!(tree.reread(), None, "already on its way");
}

#[test]
fn a_folder_of_another_location_is_not_revealed() {
    let mut tree = DirTree::new(at("/srv"), "/srv".into(), false);
    assert!(tree.reveal(&at("/home")).is_empty());
    assert!(!tree.revealing());
}
