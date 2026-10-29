use super::*;
use crate::Viewer;
use crate::test_support::{TestSource, source};

type TestViewer = Viewer<TestSource<Vec<u8>>>;

fn open(text: &str) -> TestViewer {
    Viewer::open(source(text))
        .expect("a source in memory opens")
        .0
}

/// Label, value and whether a node unfolds.
fn shown(nodes: &[TreeNode]) -> Vec<(String, Option<String>, bool)> {
    nodes
        .iter()
        .map(|node| (node.label.clone(), node.value.clone(), node.has_children))
        .collect()
}

fn row(label: &str, value: &str, has_children: bool) -> (String, Option<String>, bool) {
    (label.to_owned(), Some(value.to_owned()), has_children)
}

fn bare(label: &str) -> (String, Option<String>, bool) {
    (label.to_owned(), None, false)
}

fn expand(viewer: &mut TestViewer, nodes: &[TreeNode], label: &str) -> Vec<TreeNode> {
    let node = nodes
        .iter()
        .find(|node| node.label == label)
        .expect("the node is listed");
    viewer.children(Some(node.id)).expect("children")
}

#[test]
fn nested_mappings_and_sequences_unfold_level_by_level() {
    let mut viewer = open(
        "name: rc\nserver:\n  host: example.net\n  ports:\n    - 80\n    - 443\n  tls:\n    enabled: true\nempty:\n",
    );
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![
            row("name", "rc", false),
            row("server", "{3}", true),
            bare("empty")
        ]
    );
    let server = expand(&mut viewer, &top, "server");
    assert_eq!(
        shown(&server),
        vec![
            row("host", "example.net", false),
            row("ports", "[2]", true),
            row("tls", "{1}", true),
        ]
    );
    let ports = expand(&mut viewer, &server, "ports");
    assert_eq!(
        shown(&ports),
        vec![row("[0]", "80", false), row("[1]", "443", false)]
    );
    let tls = expand(&mut viewer, &server, "tls");
    assert_eq!(shown(&tls), vec![row("enabled", "true", false)]);
    // A scalar and an empty value have no children.
    assert!(expand(&mut viewer, &top, "name").is_empty());
    assert!(expand(&mut viewer, &top, "empty").is_empty());
    // Ids are unique.
    let mut ids: Vec<u64> = [&top, &server, &ports, &tls]
        .iter()
        .flat_map(|nodes| nodes.iter().map(|node| node.id))
        .collect();
    let all = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all);
}

#[test]
fn a_sequence_of_mappings_as_in_a_workflow() {
    let mut viewer = open(
        "steps:\n  - name: checkout\n    uses: actions/checkout@v4\n  - name: build\n    run: |\n      cargo build\n      cargo test\n  -\n    name: third\nlist:\n- a\n- b\n",
    );
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![row("steps", "[3]", true), row("list", "[2]", true)]
    );
    let steps = expand(&mut viewer, &top, "steps");
    assert_eq!(
        shown(&steps),
        vec![
            row("[0]", "{2}", true),
            row("[1]", "{2}", true),
            row("[2]", "{1}", true),
        ]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &steps, "[1]")),
        vec![
            row("name", "build", false),
            row("run", "| cargo build\u{2026}", false),
        ]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &steps, "[2]")),
        vec![row("name", "third", false)]
    );
    // An indentless sequence: its items at its key's column.
    assert_eq!(
        shown(&expand(&mut viewer, &top, "list")),
        vec![row("[0]", "a", false), row("[1]", "b", false)]
    );
}

#[test]
fn a_flow_collection_is_one_node_with_its_text() {
    let mut viewer = open("list: [a, b, {c: d}]\nmap: {x: 1, y: [2, 3]}\nmulti: [one,\n  two]\n");
    assert_eq!(
        shown(&viewer.children(None).expect("top level")),
        vec![
            row("list", "[a, b, {c: d}]", false),
            row("map", "{x: 1, y: [2, 3]}", false),
            row("multi", "[one,\u{2026}", false),
        ]
    );
}

#[test]
fn several_documents_are_nodes_of_their_own() {
    let mut viewer = open("---\na: 1\n---\n- x\n- y\n--- just text\n...\n---\n");
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![
            row("document 1", "{1}", true),
            row("document 2", "[2]", true),
            row("document 3", "just text", false),
            bare("document 4"),
        ]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &top, "document 1")),
        vec![row("a", "1", false)]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &top, "document 2")),
        vec![row("[0]", "x", false), row("[1]", "y", false)]
    );
    // One document shows its root's children, a marker or not.
    assert_eq!(
        shown(&open("---\na: 1\n").children(None).expect("top level")),
        vec![row("a", "1", false)]
    );
}

#[test]
fn a_scalar_root_is_one_node_and_nothing_is_none() {
    assert_eq!(
        shown(&open("just a string\n").children(None).expect("top")),
        vec![row("value", "just a string", false)]
    );
    assert!(open("").children(None).expect("top").is_empty());
    assert!(
        open("# a comment\n")
            .children(None)
            .expect("top")
            .is_empty()
    );
}

#[test]
fn anchors_and_aliases_are_shown_not_resolved() {
    let mut viewer = open("base: &base\n  a: 1\nuse: *base\nmerged:\n  <<: *base\n  b: 2\n");
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![
            row("base", "{1}", true),
            row("use", "*base", false),
            row("merged", "{2}", true),
        ]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &top, "merged")),
        vec![row("<<", "*base", false), row("b", "2", false)]
    );
}

#[test]
fn an_indentation_error_is_a_first_node_and_what_came_before_is_listed() {
    let mut viewer = open("a:\n    b: 1\n  c: 2\nd: 3\n");
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![
            row("invalid YAML", "line 3, column 3: bad indentation", false),
            row("a", "{1}", true),
        ]
    );
    assert_eq!(top[0].id, ERROR_NODE);
    assert!(viewer.children(Some(ERROR_NODE)).expect("none").is_empty());
    assert_eq!(
        shown(&expand(&mut viewer, &top, "a")),
        vec![row("b", "1", false)]
    );
    let mut tabbed = open("a: 1\n\tb: 2\n");
    assert_eq!(
        shown(&tabbed.children(None).expect("top level")),
        vec![
            row(
                "invalid YAML",
                "line 2, column 1: tab in the indentation",
                false
            ),
            row("a", "1", false),
        ]
    );
}

#[test]
fn a_long_sequence_unfolds_in_ranges() {
    let text = format!("items:\n{}", "  - i\n".repeat(2500));
    let mut viewer = open(&text);
    let top = viewer.children(None).expect("top level");
    assert_eq!(shown(&top), vec![row("items", "[2500]", true)]);
    let ranges = expand(&mut viewer, &top, "items");
    let labels: Vec<&str> = ranges.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(labels, vec!["[0..999]", "[1000..1999]", "[2000..2499]"]);
    let middle = viewer.children(Some(ranges[1].id)).expect("a page");
    assert_eq!(middle.len(), 1000);
    assert_eq!(middle[0].label, "[1000]");
    assert_eq!(middle[999].label, "[1999]");
    let last = viewer.children(Some(ranges[2].id)).expect("a page");
    assert_eq!(last.len(), 500);
    // The same range twice is the same node.
    assert_eq!(expand(&mut viewer, &top, "items"), ranges);
}
