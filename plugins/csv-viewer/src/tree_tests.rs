use super::*;
use crate::Viewer;
use crate::test_support::{TestSource, source};
use crate::types::{HeaderSetting, Settings};

fn open(text: &str, header: HeaderSetting) -> Viewer<TestSource<Vec<u8>>> {
    let settings = Settings {
        header,
        ..Settings::default()
    };
    Viewer::open(source(text), settings).expect("opens").0
}

/// Label and value of each node.
fn shown(nodes: &[TreeNode]) -> Vec<(String, String)> {
    nodes
        .iter()
        .map(|node| (node.label.clone(), node.value.clone().unwrap_or_default()))
        .collect()
}

fn pairs(want: &[(&str, &str)]) -> Vec<(String, String)> {
    want.iter()
        .map(|(label, value)| ((*label).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn a_node_per_record_and_the_header_names_the_fields() {
    let mut viewer = open(
        "name,city\nAda,London\nAlan,Wilmslow\n",
        HeaderSetting::Auto,
    );
    assert!(viewer.has_header());
    let rows = viewer.children(None).expect("rows");
    assert_eq!(
        shown(&rows),
        pairs(&[("row 1", "Ada | London"), ("row 2", "Alan | Wilmslow")])
    );
    assert!(rows.iter().all(|row| row.has_children));
    let fields = viewer.children(Some(rows[1].id)).expect("fields");
    assert_eq!(
        shown(&fields),
        pairs(&[("name", "Alan"), ("city", "Wilmslow")])
    );
    assert!(fields.iter().all(|field| !field.has_children));
    assert!(
        viewer
            .children(Some(fields[0].id))
            .expect("a field")
            .is_empty()
    );
}

#[test]
fn without_a_header_the_first_record_is_row_1_and_columns_are_numbered() {
    let mut viewer = open("1;2\n3;4\n", HeaderSetting::Auto);
    assert!(!viewer.has_header());
    let rows = viewer.children(None).expect("rows");
    assert_eq!(
        shown(&rows),
        pairs(&[("row 1", "1 | 2"), ("row 2", "3 | 4")])
    );
    let fields = viewer.children(Some(rows[0].id)).expect("fields");
    assert_eq!(
        shown(&fields),
        pairs(&[("column 1", "1"), ("column 2", "2")])
    );
    // `no` keeps a header-like first record as data.
    let mut forced = open("a,b\nc,d\n", HeaderSetting::No);
    assert_eq!(forced.row_count(), 2);
    assert_eq!(
        shown(&forced.children(None).expect("rows"))[0],
        ("row 1".to_owned(), "a | b".to_owned())
    );
}

#[test]
fn a_field_over_several_lines_is_one_field() {
    let text = "id,note\r\n1,\"first line\r\nsecond, \"\"quoted\"\"\"\r\n2,x\r\n";
    let mut viewer = open(text, HeaderSetting::Yes);
    let rows = viewer.children(None).expect("rows");
    assert_eq!(rows.len(), 2);
    let fields = viewer.children(Some(rows[0].id)).expect("fields");
    // A line break in a value is shown as a space.
    assert_eq!(
        shown(&fields)[1],
        (
            "note".to_owned(),
            "first line  second, \"quoted\"".to_owned()
        )
    );
    assert_eq!(shown(&rows)[1], ("row 2".to_owned(), "2 | x".to_owned()));
}

#[test]
fn records_of_unequal_length_show_what_they_have() {
    let mut viewer = open("a,b\n1\n2,3,4\n", HeaderSetting::Yes);
    let rows = viewer.children(None).expect("rows");
    let short = viewer.children(Some(rows[0].id)).expect("fields");
    assert_eq!(shown(&short), pairs(&[("a", "1")]));
    let long = viewer.children(Some(rows[1].id)).expect("fields");
    assert_eq!(
        shown(&long),
        pairs(&[("a", "2"), ("b", "3"), ("column 3", "4")])
    );
}

#[test]
fn a_value_is_cut_short() {
    let long = "x".repeat(200);
    let mut viewer = open(&format!("{long},b\n"), HeaderSetting::No);
    let rows = viewer.children(None).expect("rows");
    let value = rows[0].value.clone().unwrap_or_default();
    assert_eq!(value.chars().count(), PREVIEW_CHARS + 1);
    assert!(value.ends_with('\u{2026}'));
    let fields = viewer.children(Some(rows[0].id)).expect("fields");
    assert_eq!(
        fields[0]
            .value
            .as_deref()
            .map(|value| value.chars().count()),
        Some(PREVIEW_CHARS + 1)
    );
}

#[test]
fn many_records_unfold_in_ranges() {
    let mut text = String::from("n,square\n");
    for n in 0..2500u64 {
        text.push_str(&format!("{n},{}\n", n * n));
    }
    let mut viewer = open(&text, HeaderSetting::Auto);
    assert_eq!(viewer.row_count(), 2500);
    let ranges = viewer.children(None).expect("ranges");
    assert_eq!(
        ranges
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        vec!["rows 1..1000", "rows 1001..2000", "rows 2001..2500"]
    );
    let page = viewer.children(Some(ranges[1].id)).expect("a page");
    assert_eq!(page.len(), 1000);
    assert_eq!(
        shown(&page)[0],
        ("row 1001".to_owned(), "1000 | 1000000".to_owned())
    );
    let last = viewer.children(Some(ranges[2].id)).expect("the last page");
    assert_eq!(
        shown(&last)[499],
        ("row 2500".to_owned(), "2499 | 6245001".to_owned())
    );
    let fields = viewer.children(Some(last[499].id)).expect("fields");
    assert_eq!(
        shown(&fields),
        pairs(&[("n", "2499"), ("square", "6245001")])
    );
    // The same range keeps its id.
    assert_eq!(viewer.children(None).expect("ranges"), ranges);
}

#[test]
fn a_record_of_many_fields_unfolds_in_ranges() {
    let record: Vec<String> = (0..1500).map(|n| n.to_string()).collect();
    let mut viewer = open(&format!("{}\n", record.join(",")), HeaderSetting::No);
    let rows = viewer.children(None).expect("rows");
    let ranges = viewer.children(Some(rows[0].id)).expect("ranges");
    assert_eq!(
        ranges
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        vec!["columns 1..1000", "columns 1001..1500"]
    );
    let fields = viewer.children(Some(ranges[1].id)).expect("fields");
    assert_eq!(fields.len(), 500);
    assert_eq!(
        shown(&fields)[0],
        ("column 1001".to_owned(), "1000".to_owned())
    );
}

#[test]
fn an_unknown_id_is_not_found() {
    let mut viewer = open("a\n", HeaderSetting::No);
    let failure = viewer.children(Some(5)).expect_err("no row 6");
    assert_eq!(failure.kind, FailureKind::NotFound);
    let failure = viewer.children(Some(RANGE_BIT | 7)).expect_err("no range");
    assert_eq!(failure.kind, FailureKind::NotFound);
}

#[test]
fn preview_keeps_one_line() {
    assert_eq!(preview("a\tb\nc", false), "a b c");
    assert_eq!(preview("ab", true), "ab\u{2026}");
}
