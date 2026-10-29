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

fn row(label: &str, value: Option<&str>, has_children: bool) -> (String, Option<String>, bool) {
    (label.to_owned(), value.map(str::to_owned), has_children)
}

fn expand(viewer: &mut TestViewer, nodes: &[TreeNode], label: &str) -> Vec<TreeNode> {
    let node = nodes
        .iter()
        .find(|node| node.label == label)
        .expect("the node is listed");
    viewer.children(Some(node.id)).expect("children")
}

#[test]
fn nested_elements_unfold_level_by_level() {
    let mut viewer = open(
        "<?xml version=\"1.0\"?>\n<!-- top -->\n<project Sdk=\"Microsoft.NET.Sdk\">\n  \
         <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n    \
         <Nullable>enable</Nullable>\n  </PropertyGroup>\n  <!-- skipped -->\n  \
         <ItemGroup><Ref a='1' b=\"x &amp; y\"/></ItemGroup>\n</project>\n",
    );
    let top = viewer.children(None).expect("top level");
    assert_eq!(
        shown(&top),
        vec![row("<project Sdk=\"Microsoft.NET.Sdk\">", None, true)]
    );
    let project = expand(&mut viewer, &top, "<project Sdk=\"Microsoft.NET.Sdk\">");
    assert_eq!(
        shown(&project),
        vec![
            row("<PropertyGroup>", None, true),
            row("<ItemGroup>", None, true)
        ]
    );
    let group = expand(&mut viewer, &project, "<PropertyGroup>");
    assert_eq!(
        shown(&group),
        vec![
            row("<TargetFramework>", Some("net8.0"), false),
            row("<Nullable>", Some("enable"), false),
        ]
    );
    let items = expand(&mut viewer, &project, "<ItemGroup>");
    assert_eq!(
        shown(&items),
        vec![row("<Ref a='1' b=\"x &amp; y\"/>", None, false)]
    );
    // An element without children has none to list.
    assert!(expand(&mut viewer, &group, "<Nullable>").is_empty());
}

#[test]
fn empty_elements_are_leaves_without_a_value() {
    let mut viewer = open("<a><b/><c></c><d>  </d><e x=\"1\" /></a>");
    let top = viewer.children(None).expect("top");
    assert_eq!(shown(&top), vec![row("<a>", None, true)]);
    assert_eq!(
        shown(&expand(&mut viewer, &top, "<a>")),
        vec![
            row("<b/>", None, false),
            row("<c>", None, false),
            row("<d>", None, false),
            row("<e x=\"1\"/>", None, false),
        ]
    );
    let mut alone = open("<only/>");
    assert_eq!(
        shown(&alone.children(None).expect("top")),
        vec![row("<only/>", None, false)]
    );
}

#[test]
fn a_value_is_the_folded_decoded_text_of_a_leaf() {
    let mut viewer = open(
        "<r><t>  one\n   two &lt;3&gt; &#233;&#x41; &custom;</t><c><![CDATA[a <b>\n c]]></c>\
         <m>mixed<x/>text</m></r>",
    );
    let top = viewer.children(None).expect("top");
    assert_eq!(
        shown(&expand(&mut viewer, &top, "<r>")),
        vec![
            row("<t>", Some("one two <3> \u{e9}A &custom;"), false),
            row("<c>", Some("a <b> c"), false),
            row("<m>", None, true),
        ]
    );
}

#[test]
fn long_labels_and_values_are_cut() {
    let text = format!(
        "<r><e a=\"{}\">{}</e></r>",
        "v".repeat(200),
        "w".repeat(10_000)
    );
    let mut viewer = open(&text);
    let top = viewer.children(None).expect("top");
    let leaf = &expand(&mut viewer, &top, "<r>")[0];
    assert_eq!(leaf.label.chars().count(), crate::walk::PREVIEW_CHARS + 1);
    assert!(leaf.label.starts_with("<e a=\"vvv") && leaf.label.ends_with('\u{2026}'));
    let value = leaf.value.clone().expect("a value");
    assert_eq!(value.chars().count(), crate::walk::PREVIEW_CHARS + 1);
    assert!(value.ends_with('\u{2026}'));
}

#[test]
fn many_siblings_are_shown_in_pages() {
    let items: String = (0..2500).map(|index| format!("<i>{index}</i>")).collect();
    let mut viewer = open(&format!("<list>{items}</list>"));
    let top = viewer.children(None).expect("top");
    let ranges = expand(&mut viewer, &top, "<list>");
    let labels: Vec<&str> = ranges.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(labels, vec!["[0..999]", "[1000..1999]", "[2000..2499]"]);
    assert!(
        ranges
            .iter()
            .all(|node| node.has_children && node.value.is_none())
    );
    let page = expand(&mut viewer, &ranges, "[1000..1999]");
    assert_eq!(page.len(), 1000);
    assert_eq!(shown(&page[..1]), vec![row("<i>", Some("1000"), false)]);
    assert_eq!(shown(&page[999..]), vec![row("<i>", Some("1999"), false)]);
    assert_eq!(expand(&mut viewer, &ranges, "[2000..2499]").len(), 500);
    // Asking again hands out the same ranges.
    assert_eq!(viewer.children(Some(top[0].id)).expect("again"), ranges);
}

#[test]
fn a_page_deep_in_a_huge_list_is_read_from_its_anchor() {
    let text = format!("<r>{}</r>", "<i/>".repeat(1_500_000));
    let mut viewer = open(&text);
    let top = viewer.children(None).expect("top");
    let ranges = expand(&mut viewer, &top, "<r>");
    let labels: Vec<&str> = ranges.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(labels, vec!["[0..999999]", "[1000000..1499999]"]);
    let thousands = expand(&mut viewer, &ranges, "[1000000..1499999]");
    assert_eq!(thousands.len(), 500);
    assert_eq!(thousands[123].label, "[1123000..1123999]");
    viewer.src.reset_counts();
    let page = viewer.children(Some(thousands[123].id)).expect("one page");
    assert_eq!(page.len(), 1000);
    assert_eq!(page[0].id, 3 + 4 * 1_123_000);
    // One window around the anchor, not the 4 MB before it.
    assert!(
        viewer.src.bytes_read <= 2 * 64 * 1024,
        "{}",
        viewer.src.bytes_read
    );
}

#[test]
fn broken_xml_shows_its_first_error_and_what_came_before() {
    let mut viewer = open("<a>\n  <b>1</b>\n  <c><d>2</d></e>\n  <f/>\n</a>");
    let top = viewer.children(None).expect("top");
    assert_eq!(
        shown(&top),
        vec![
            row(
                "invalid XML",
                Some("line 3, column 14: expected </c>, found </e>"),
                false
            ),
            row("<a>", None, true),
        ]
    );
    assert_eq!(top[0].id, ERROR_NODE);
    assert!(viewer.children(Some(ERROR_NODE)).expect("none").is_empty());
    let a = expand(&mut viewer, &top, "<a>");
    assert_eq!(
        shown(&a),
        vec![row("<b>", Some("1"), false), row("<c>", None, true)]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &a, "<c>")),
        vec![row("<d>", Some("2"), false)]
    );
}

#[test]
fn an_unclosed_document_lists_up_to_its_end() {
    let mut viewer = open("<a><b>x</b><c>");
    let top = viewer.children(None).expect("top");
    assert_eq!(
        shown(&top),
        vec![
            row(
                "invalid XML",
                Some("line 1, column 12: <c> is not closed"),
                false
            ),
            row("<a>", None, true),
        ]
    );
    assert_eq!(
        shown(&expand(&mut viewer, &top, "<a>")),
        vec![row("<b>", Some("x"), false), row("<c>", None, false)]
    );
    let mut nothing = open("just text");
    assert_eq!(
        shown(&nothing.children(None).expect("top")),
        vec![row(
            "invalid XML",
            Some("line 1, column 1: text outside the root element"),
            false
        )]
    );
}

#[test]
fn an_unknown_range_is_not_found() {
    let mut viewer = open("<a><b/></a>");
    let failure = viewer
        .children(Some(RANGE_BIT | 12345))
        .expect_err("no such range");
    assert_eq!(failure.kind, FailureKind::NotFound);
    // An offset where no element starts has no children.
    assert!(viewer.children(Some(1)).expect("inside a tag").is_empty());
    assert!(viewer.children(Some(999)).expect("past the end").is_empty());
}
