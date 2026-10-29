//! The tree's model: what it takes of an answer, its limits, the cursor and the rows on screen.

use super::{MAX_DEPTH, MAX_NODES, MAX_TEXT_BYTES, Step, Tree};
use crate::app::viewer::document::TreeNode;

fn node(id: u64, has_children: bool) -> TreeNode {
    TreeNode {
        id,
        label: format!("node {id}"),
        value: Some(id.to_string()),
        has_children,
    }
}

fn ids(tree: &Tree) -> Vec<u64> {
    tree.rows.iter().map(|row| row.id).collect()
}

/// A tree with its top level `top` in.
fn tree_of(top: Vec<TreeNode>) -> Tree {
    let mut tree = Tree::default();
    assert_eq!(tree.start(), Some(None));
    assert_eq!(tree.start(), None, "the top level is asked once");
    assert!(tree.loading());
    tree.arrived(None, Ok(top)).expect("taken");
    assert!(!tree.loading());
    tree
}

#[test]
fn an_answer_nobody_waits_for_is_dropped_and_a_failed_one_may_be_asked_again() {
    let mut tree = tree_of(vec![node(1, true)]);
    tree.arrived(Some(1), Ok(vec![node(2, false)]))
        .expect("dropped quietly");
    assert_eq!(ids(&tree), [1], "node 1 did not ask");
    tree.arrived(None, Ok(vec![node(9, false)]))
        .expect("dropped quietly");
    assert_eq!(ids(&tree), [1], "the top level is in");

    assert_eq!(tree.step(Step::Open), Some(1));
    assert_eq!(
        tree.arrived(Some(1), Err("plugin_failed")),
        Err("plugin_failed")
    );
    assert_eq!(ids(&tree), [1]);
    assert_eq!(tree.step(Step::Open), Some(1), "asked again");
}

#[test]
fn labels_and_values_stay_on_one_bounded_line() {
    let long = TreeNode {
        id: 1,
        label: format!("a\nb\t{}", "\u{e9}".repeat(MAX_TEXT_BYTES)),
        value: Some("x\r\ny".to_owned()),
        has_children: false,
    };
    let tree = tree_of(vec![long]);
    let shown = tree.shown(tree.rows[0]).expect("a row");
    assert!(shown.label.starts_with("a b "));
    assert!(shown.label.len() <= MAX_TEXT_BYTES);
    assert_eq!(shown.value, Some("x  y"));
    assert_eq!(shown.open, None, "a leaf");
}

#[test]
fn no_node_unfolds_below_the_deepest_level() {
    let mut tree = tree_of(vec![node(0, true)]);
    for depth in 0..MAX_DEPTH as u64 {
        let id = depth;
        let row = tree
            .rows
            .iter()
            .position(|row| row.id == id)
            .expect("shown");
        assert_eq!(tree.click(row), Some(id), "level {depth} unfolds");
        tree.arrived(Some(id), Ok(vec![node(id + 1, true)]))
            .expect("taken");
    }
    let deepest = tree.rows.last().copied().expect("rows");
    assert_eq!(deepest.depth, MAX_DEPTH);
    assert_eq!(tree.shown(deepest).expect("shown").open, None, "a leaf now");
}

#[test]
fn the_tree_stops_growing_at_its_limit() {
    let top: Vec<TreeNode> = (0..MAX_NODES as u64 + 5)
        .map(|id| node(id, false))
        .collect();
    let tree = tree_of(top);
    assert_eq!(tree.len(), MAX_NODES);
    assert!(tree.full);
}

#[test]
fn the_cursor_moves_by_page_and_keeps_on_screen() {
    let mut tree = tree_of((0..50).map(|id| node(id, false)).collect());
    tree.set_height(10);
    assert_eq!(tree.visible().len(), 10);
    tree.step(Step::PageDown);
    assert_eq!(tree.cursor, 9);
    tree.step(Step::Down);
    assert_eq!((tree.cursor, tree.top), (10, 1));
    tree.step(Step::End);
    assert_eq!((tree.cursor, tree.top), (49, 40));
    tree.step(Step::Home);
    assert_eq!((tree.cursor, tree.top), (0, 0));
    tree.scroll(100);
    assert_eq!(tree.top, 40, "the wheel stops at the end");
    tree.scroll(-3);
    assert_eq!(tree.top, 37);
    assert_eq!(tree.cursor, 0, "the wheel leaves the cursor");
    assert!(
        tree.step(Step::Close).is_none(),
        "nothing to fold at the top"
    );
}
