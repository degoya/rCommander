use super::*;
use crate::Viewer;
use crate::test_support::{TestSource, source};
use crate::types::Format;

type TestViewer = Viewer<TestSource<Vec<u8>>>;

fn open(text: &str) -> TestViewer {
    Viewer::open(source(text), Format::AsIs)
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

fn expand(viewer: &mut TestViewer, nodes: &[TreeNode], label: &str) -> Vec<TreeNode> {
    let node = nodes
        .iter()
        .find(|node| node.label == label)
        .expect("the node is listed");
    viewer.children(Some(node.id)).expect("children")
}

#[test]
fn nested_values_unfold_level_by_level() {
    let mut viewer = open(
        r#"{"name": "rc", "list": [1, {"deep": [true, null]}, []], "empty": {}, "n": -2.5e3}"#,
    );
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![
            row("name", "\"rc\"", false),
            row("list", "[3]", true),
            row("empty", "{0}", false),
            row("n", "-2.5e3", false),
        ]
    );
    let list = expand(&mut viewer, &top, "list");
    assert_eq!(
        shown(&list),
        vec![
            row("[0]", "1", false),
            row("[1]", "{1}", true),
            row("[2]", "[0]", false)
        ]
    );
    let object = expand(&mut viewer, &list, "[1]");
    assert_eq!(shown(&object), vec![row("deep", "[2]", true)]);
    let deep = expand(&mut viewer, &object, "deep");
    assert_eq!(
        shown(&deep),
        vec![row("[0]", "true", false), row("[1]", "null", false)]
    );
    // A scalar and an empty container have no children.
    assert!(expand(&mut viewer, &top, "name").is_empty());
    assert!(expand(&mut viewer, &top, "empty").is_empty());
}

#[test]
fn empty_documents_have_an_empty_top_level() {
    assert!(open("{}").children(None).expect("top").is_empty());
    assert!(open(" [ ] ").children(None).expect("top").is_empty());
}

#[test]
fn a_scalar_root_is_one_node() {
    let mut viewer = open("\"just a string\"");
    assert_eq!(
        shown(&viewer.children(None).expect("top")),
        vec![row("value", "\"just a string\"", false)]
    );
}

#[test]
fn a_long_list_is_shown_in_pages() {
    let items: Vec<String> = (0..2500).map(|index| index.to_string()).collect();
    let mut viewer = open(&format!("[{}]", items.join(",")));
    let top = viewer.children(None).expect("top");
    let labels: Vec<&str> = top.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(labels, vec!["[0..999]", "[1000..1999]", "[2000..2499]"]);
    assert!(
        top.iter()
            .all(|node| node.has_children && node.value.is_none())
    );
    let page = expand(&mut viewer, &top, "[1000..1999]");
    assert_eq!(page.len(), 1000);
    assert_eq!(shown(&page[..1]), vec![row("[1000]", "1000", false)]);
    assert_eq!(shown(&page[999..]), vec![row("[1999]", "1999", false)]);
    assert_eq!(expand(&mut viewer, &top, "[2000..2499]").len(), 500);
    // Asking again hands out the same ranges.
    assert_eq!(viewer.children(None).expect("top again"), top);
}

#[test]
fn a_page_deep_in_a_huge_list_is_read_from_its_anchor() {
    let text = format!("[{}0]", "0,".repeat(1_499_999));
    let mut viewer = open(&text);
    let top = viewer.children(None).expect("top");
    let labels: Vec<&str> = top.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(labels, vec!["[0..999999]", "[1000000..1499999]"]);
    let thousands = expand(&mut viewer, &top, "[1000000..1499999]");
    assert_eq!(thousands.len(), 500);
    assert_eq!(thousands[123].label, "[1123000..1123999]");
    viewer.src.reset_counts();
    let page = viewer.children(Some(thousands[123].id)).expect("one page");
    assert_eq!(page.len(), 1000);
    assert_eq!(page[0].label, "[1123000]");
    // One window around the anchor, not the 3 MB before it.
    assert!(
        viewer.src.bytes_read <= 2 * 64 * 1024,
        "{}",
        viewer.src.bytes_read
    );
}

#[test]
fn a_long_string_is_previewed_and_jumped_over() {
    let text = format!(r#"{{"big": "{}", "after": 1}}"#, "x".repeat(1 << 20));
    let mut viewer = open(&text);
    viewer.src.reset_counts();
    let top = viewer.children(None).expect("top");
    assert_eq!(top.len(), 2);
    let big = top[0].value.clone().expect("a preview");
    assert_eq!(big.chars().count(), crate::walk::PREVIEW_CHARS + 2);
    assert!(big.starts_with("\"xxx") && big.ends_with('\u{2026}'));
    assert_eq!(shown(&top[1..]), vec![row("after", "1", false)]);
    assert!(
        viewer.src.bytes_read < 256 * 1024,
        "{}",
        viewer.src.bytes_read
    );
}

#[test]
fn invalid_json_shows_its_first_error_and_what_came_before() {
    let mut viewer = open("{\"a\": 1,\n \"b\": [true, fals],\n \"c\": 3}");
    let top = viewer.children(None).expect("top");
    assert_eq!(
        shown(&top),
        vec![
            row("invalid JSON", "line 2, column 14: invalid literal", false),
            row("a", "1", false),
            row("b", "[1]", true),
        ]
    );
    assert_eq!(top[0].id, ERROR_NODE);
    assert!(viewer.children(Some(ERROR_NODE)).expect("none").is_empty());
    assert_eq!(
        shown(&expand(&mut viewer, &top, "b")),
        vec![row("[0]", "true", false)]
    );
}

#[test]
fn a_truncated_file_lists_up_to_its_end() {
    let mut viewer = open(r#"[1, 2, {"k": "v""#);
    let top = viewer.children(None).expect("top");
    assert_eq!(
        shown(&top),
        vec![
            row(
                "invalid JSON",
                "line 1, column 17: unexpected end of file",
                false
            ),
            row("[0]", "1", false),
            row("[1]", "2", false),
            row("[2]", "{1}", true),
        ]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &top, "[2]")),
        vec![row("k", "\"v\"", false)]
    );
    let mut viewer = open(r#"{"a": 1, "b": "#);
    let top = viewer.children(None).expect("top");
    assert_eq!(top.len(), 3);
    assert_eq!(
        (top[2].label.as_str(), top[2].value.as_deref()),
        ("b", None)
    );
}

#[test]
fn an_unknown_range_is_not_found() {
    let mut viewer = open("[1]");
    let failure = viewer
        .children(Some(RANGE_BIT | 12345))
        .expect_err("no such range");
    assert_eq!(failure.kind, FailureKind::NotFound);
    // An offset that is no container has no children.
    assert!(viewer.children(Some(1)).expect("a scalar").is_empty());
    assert!(viewer.children(Some(999)).expect("past the end").is_empty());
}
