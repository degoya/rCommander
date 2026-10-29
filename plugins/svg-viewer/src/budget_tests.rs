use super::*;
use crate::code;
use crate::render::render;
use crate::settings::Settings;
use crate::test_support::{nested, use_bomb};

fn count(svg: &str, limit: u64) -> u64 {
    let doc = Document::parse(svg).expect("the test SVG parses");
    expanded_elements(&doc, limit)
}

#[test]
fn elements_without_use_count_once() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g><rect/><circle/></g><path/></svg>"#;
    assert_eq!(count(svg, 100), 5);
}

#[test]
fn a_use_counts_what_it_references_each_time() {
    // `g` (3 elements) is drawn once in place and twice more through `use`, by either spelling of
    // `href`; each `use` is an element of its own.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <g id="g"><rect/><rect/></g>
        <use href="#g"/>
        <use xlink:href="#g"/>
    </svg>"##;
    assert_eq!(count(svg, 100), 1 + 3 + (1 + 3) + (1 + 3));
}

#[test]
fn a_reference_to_nothing_or_outside_counts_only_the_use() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
        <use href="#missing"/><use href="other.svg#g"/><use/>
    </svg>"##;
    assert_eq!(count(svg, 100), 4);
}

#[test]
fn a_cycle_ends() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
        <g id="a"><use href="#b"/></g>
        <g id="b"><use href="#a"/></g>
        <g id="c"><use href="#c"/></g>
    </svg>"##;
    let total = count(svg, 100);
    assert!(total > 0 && total <= 100, "{total}");
}

#[test]
fn a_use_bomb_is_over_the_budget_without_being_expanded() {
    // Ten levels of ten references: 10^10 elements once expanded, a few kilobytes as written.
    let svg = use_bomb(10, 10);
    assert!(svg.len() < 8 << 10);
    assert_eq!(
        count(&svg, MAX_EXPANDED_ELEMENTS),
        MAX_EXPANDED_ELEMENTS + 1
    );
}

#[test]
fn the_limit_is_inclusive() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect/><rect/></svg>"#;
    assert_eq!(count(svg, 3), 3);
    assert_eq!(count(svg, 2), 3);
    assert_eq!(count(svg, 1), 2);
}

#[test]
fn a_deep_document_is_refused_before_roxmltree_parses_it() {
    // roxmltree recurses once per level, so 100 000 levels would overflow the stack in
    // `Document::parse`, before any count; the depth budget refuses the document first.
    let svg = nested(100_000);
    let failure = render(svg.as_bytes(), Settings::default()).expect_err("too deep");
    assert_eq!(failure.code.as_deref(), Some(code::TOO_COMPLEX));
}
