use super::*;
use crate::test_support::nested;

#[test]
fn open_elements_are_counted_the_root_included() {
    let text = b"<svg><g><g></g><rect/></g><g/></svg>";
    assert!(depth_within(text, 3));
    assert!(!depth_within(text, 2));
}

#[test]
fn the_budget_is_inclusive() {
    assert!(depth_within(nested(MAX_DEPTH).as_bytes(), MAX_DEPTH));
    assert!(!depth_within(nested(MAX_DEPTH + 1).as_bytes(), MAX_DEPTH));
    // The 1024 levels the coordinator named as a scanner case: told, and over the budget.
    assert!(depth_within(nested(1024).as_bytes(), 1024));
    assert!(!depth_within(nested(1024).as_bytes(), MAX_DEPTH));
}

#[test]
fn a_deep_document_is_refused_by_the_scan_alone() {
    assert!(!depth_within(nested(100_000).as_bytes(), MAX_DEPTH));
}

#[test]
fn markup_that_is_no_element_counts_nothing() {
    let text = br#"<?xml version="1.0"?>
<!DOCTYPE svg [
  <!ENTITY e "<g><g>">
  <!-- ] > <g> -->
  <!ATTLIST svg a CDATA '<g>'>
]>
<!-- <g><g><g> -->
<svg a="<g>" b='x > <g>' c="/">
  <?pi <g><g> ?>
  <![CDATA[ <g><g><g> ]]>
  <rect /><circle
     r="1"/>
</svg>"#;
    assert!(depth_within(text, 1));
    assert!(!depth_within(text, 0));
}

#[test]
fn closing_more_than_was_opened_does_not_wrap() {
    assert!(depth_within(b"</g></g></g><svg><g/></svg>", 1));
}

#[test]
fn an_unclosed_end_counts_what_was_seen() {
    assert!(depth_within(b"<svg><g", 2));
    assert!(!depth_within(b"<svg><g", 1));
    assert!(depth_within(b"<svg><!-- never closed <g><g>", 1));
    assert!(depth_within(b"", 0));
}
