use super::*;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < f32::EPSILON
}

/// A row is indented a level per submenu above it, as the connection manager's tree.
#[test]
fn each_level_indents_as_the_connection_tree_does() {
    assert!(near(indent(0), 0.0));
    assert!(near(indent(1), 16.0));
    assert!(near(indent(3), 48.0));
}
