use super::{MAX_TREE_CHILDREN, MAX_TREE_DEPTH, TreeState};
use crate::bindings::exports::rcommander::plugin::viewer as wit;
use crate::failure::codes;

fn node(id: u64, label: &str) -> wit::TreeNode {
    wit::TreeNode {
        id,
        label: label.to_owned(),
        value: Some("v\u{7}".to_owned()),
        has_children: true,
    }
}

#[test]
fn only_nodes_handed_out_are_expanded() {
    let mut tree = TreeState::default();
    assert_eq!(
        tree.depth_below(Some(7))
            .expect_err("never handed out")
            .code(),
        Some(codes::PLUGIN_UNKNOWN_NODE)
    );
    let roots = tree.accept(0, vec![node(7, "root"), node(7, "again")]);
    assert_eq!(roots.nodes.len(), 1, "an id twice is dropped");
    assert!(roots.truncated);
    assert_eq!(roots.nodes[0].value.as_deref(), Some("v\\u{7}"));
    assert_eq!(tree.depth_below(Some(7)).expect("known"), 1);
}

#[test]
fn a_million_children_are_cut_to_the_cap() {
    let mut tree = TreeState::default();
    let answered: Vec<_> = (0..1_000_000).map(|id| node(id, "")).collect();
    let shown = tree.accept(0, answered);
    assert_eq!(shown.nodes.len(), MAX_TREE_CHILDREN);
    assert!(shown.truncated);
}

#[test]
fn the_tree_stops_at_its_depth() {
    let mut tree = TreeState::default();
    let mut parent = None;
    for id in 0..=u64::from(MAX_TREE_DEPTH) {
        let depth = tree.depth_below(parent).expect("within the depth");
        let children = tree.accept(depth, vec![node(id, "level")]);
        assert_eq!(children.nodes[0].has_children, depth < MAX_TREE_DEPTH);
        parent = Some(id);
    }
    assert_eq!(
        tree.depth_below(parent).expect_err("too deep").code(),
        Some(codes::PLUGIN_TREE_TOO_DEEP)
    );
}
