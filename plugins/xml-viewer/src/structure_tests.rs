use super::*;
use crate::Viewer;
use crate::test_support::source;
use crate::types::Format;

/// The first error of `text`: line, column and message.
fn error(text: &str) -> Option<(u64, u64, String)> {
    let (viewer, _) = Viewer::open(source(text), Format::AsIs).expect("a source in memory opens");
    viewer
        .parse_error()
        .map(|error| (error.line, error.column, error.message.clone()))
}

fn at(line: u64, column: u64, message: &str) -> Option<(u64, u64, String)> {
    Some((line, column, message.to_owned()))
}

#[test]
fn well_formed_documents_have_no_error() {
    for text in [
        "<a/>",
        "\u{feff}<?xml version=\"1.0\"?>\n<!DOCTYPE a>\n<!-- c --><a b='1' c=\"&amp;\"/>\n<?pi x?>\n",
        "<a><b>t &lt; <![CDATA[ <x> ]]></b><c\n  d=\"multi\nline\"\n/></a>",
        "<ns:a xmlns:ns=\"u\"><ns:b/></ns:a>",
    ] {
        assert_eq!(error(text), None, "{text}");
    }
}

#[test]
fn a_wrong_end_tag_names_the_one_expected() {
    assert_eq!(
        error("<a>\n  <b></c>\n</a>"),
        at(2, 6, "expected </b>, found </c>")
    );
    assert_eq!(
        error("<a></a></a>"),
        at(1, 8, "an end tag without its start tag")
    );
}

#[test]
fn an_element_left_open_points_at_its_start_tag() {
    assert_eq!(error("<a>\n  <b>text\n"), at(2, 3, "<b> is not closed"));
    let (viewer, _) = Viewer::open(source("<a><b>"), Format::AsIs).expect("opens");
    // The well-formed part is the whole file.
    assert_eq!(viewer.parse_error().map(|error| error.offset), Some(6));
}

#[test]
fn constructs_left_open_are_named() {
    assert_eq!(error("<a><!-- c\n"), at(1, 4, "unterminated comment"));
    assert_eq!(
        error("<a><![CDATA[x"),
        at(1, 4, "unterminated CDATA section")
    );
    assert_eq!(
        error("<?xml x"),
        at(1, 1, "unterminated processing instruction")
    );
    assert_eq!(error("<!DOCTYPE a [\n"), at(1, 1, "unterminated DOCTYPE"));
    assert_eq!(
        error("<a b=\"x\ny"),
        at(1, 6, "unterminated attribute value")
    );
    assert_eq!(error("<a b=\"x"), at(1, 6, "unterminated attribute value"));
    assert_eq!(error("<a\n b='1'"), at(1, 1, "unterminated tag"));
}

#[test]
fn malformed_tags_and_documents_are_errors() {
    assert_eq!(error(""), at(1, 1, "no root element"));
    assert_eq!(error("text"), at(1, 1, "text outside the root element"));
    assert_eq!(error("<a/><b/>"), at(1, 5, "a second root element"));
    assert_eq!(
        error("<a b>"),
        at(1, 5, "expected '=' after the attribute name")
    );
    assert_eq!(
        error("<a b=c>"),
        at(1, 6, "expected a quoted attribute value")
    );
    assert_eq!(
        error("<a b=\"1\" <c/></a>"),
        at(1, 10, "expected an attribute, '>' or '/>'")
    );
    assert_eq!(
        error("<a b=\"x<c/>\"/>"),
        at(1, 8, "unterminated attribute value")
    );
    assert_eq!(error("<a>&nope</a>"), at(1, 4, "invalid entity reference"));
    assert_eq!(error("<a>< </a>"), at(1, 4, "invalid markup"));
    assert_eq!(
        error("<a/><!DOCTYPE a>"),
        at(1, 5, "a DOCTYPE after the root element began")
    );
    assert_eq!(error("<a #/>"), at(1, 4, "unexpected character in a tag"));
}

#[test]
fn a_huge_name_is_matched_by_its_kept_bytes_and_length() {
    let long = "n".repeat(NAME_BYTES + 10);
    let longer = "n".repeat(NAME_BYTES + 11);
    assert_eq!(error(&format!("<{long}></{long}>")), None);
    let message = error(&format!("<{long}></{longer}>")).expect("an error").2;
    assert!(message.starts_with("expected </nnn"), "{message}");
    assert!(message.contains('\u{2026}'), "{message}");
}

#[test]
fn nesting_deeper_than_the_limit_is_an_error() {
    let text = "<a>".repeat(MAX_DEPTH + 1);
    let found = error(&text).expect("an error");
    assert_eq!(found.2, "nesting too deep");
    assert_eq!(found.1, 3 * MAX_DEPTH as u64 + 1);
}

#[test]
fn large_and_wide_elements_are_indexed_with_anchors() {
    let items: String = (0..2500)
        .map(|index| format!("<i n=\"{index}\"/>"))
        .collect();
    let text = format!("<root>{items}</root>");
    let (viewer, _) = Viewer::open(source(&text), Format::AsIs).expect("opens");
    let structure = &viewer.structure;
    assert_eq!(structure.root, Some(0));
    let root = structure.entry(0).expect("the root is indexed");
    assert_eq!(root.count, 2500);
    assert_eq!(root.content, 6);
    assert_eq!(root.end, text.len() as u64);
    assert_eq!(root.anchors.len(), 2);
    let second = root.anchors[0] as usize;
    assert!(text[second..].starts_with("<i n=\"1000\"/>"));
    // A small element is not indexed: a walk reads it.
    assert!(structure.entry(6).is_none());
}
