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
fn an_unprefixed_href_wins_over_xlink_href_in_any_order() {
    // usvg 0.49 follows `href` when both are present (SVG 2); the count follows the same one, so
    // a small `xlink:href` cannot hide a large `href`. An `href` in another namespace is none.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
        xmlns:x="urn:other">
        <g id="big"><rect/><rect/><rect/></g>
        <g id="small"/>
        <use xlink:href="#small" href="#big"/>
        <use href="#big" xlink:href="#small"/>
        <use x:href="#big"/>
    </svg>"##;
    assert_eq!(count(svg, 100), 1 + 4 + 1 + (1 + 4) + (1 + 4) + 1);
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
    // usvg and resvg recurse once per level, so 100 000 levels would overflow the stack; the
    // depth budget refuses the document before it is parsed at all.
    let svg = nested(100_000);
    let failure = render(svg.as_bytes(), Settings::default()).expect_err("too deep");
    assert_eq!(failure.code.as_deref(), Some(code::TOO_COMPLEX));
}

#[test]
fn a_repeated_id_names_its_first_element_as_usvg_maps_it() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
        <g id="a"><rect/><rect/></g>
        <g id="a"/>
        <use href="#a"/>
    </svg>"##;
    assert_eq!(count(svg, 100), 1 + 3 + 1 + (1 + 3));
}

#[test]
fn an_empty_copy_of_each_id_after_it_hides_no_use_bomb() {
    // Each level is defined twice, the empty copy last: a map that kept the last copy would count
    // a few dozen elements where usvg builds ten billion.
    let mut svg =
        String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><defs><g id="l0"><rect/></g>"#);
    for level in 1..=10 {
        svg.push_str(&format!(r#"<g id="l{level}">"#));
        for _ in 0..10 {
            svg.push_str(&format!(r##"<use href="#l{}"/>"##, level - 1));
        }
        svg.push_str(&format!(r#"</g><g id="l{level}"/>"#));
    }
    svg.push_str(r##"</defs><use href="#l10"/></svg>"##);
    assert_eq!(
        count(&svg, MAX_EXPANDED_ELEMENTS),
        MAX_EXPANDED_ELEMENTS + 1
    );
}

#[test]
fn a_reference_is_read_as_svgtypes_reads_it() {
    assert_eq!(iri("#a"), Some("a"));
    assert_eq!(iri(" \t\r\n#a \t\r\n"), Some("a"));
    // Only a plain space ends the id; a tab after it stays part of it.
    assert_eq!(iri("#a\t"), Some("a\t"));
    for refused in ["", "#", "# a", "a", "#a b", "url(#a)"] {
        assert_eq!(iri(refused), None, "{refused:?}");
    }
}

#[test]
fn spaces_around_a_reference_hide_no_use_bomb() {
    // usvg follows `href=" #l9 "`; a count that took it for no reference would see a few dozen.
    let svg = use_bomb(10, 10).replace(r##"href="#l"##, r##"href=" #l"##);
    assert!(svg.contains(r##"href=" #l9""##));
    assert_eq!(
        count(&svg, MAX_EXPANDED_ELEMENTS),
        MAX_EXPANDED_ELEMENTS + 1
    );
}

#[test]
fn an_embedded_svg_takes_its_elements_from_what_is_left() {
    let small = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect/><rect/></svg>"#;
    let remaining = Remaining::new(7);
    assert!(remaining.take_svg(small.as_bytes()));
    assert!(remaining.take_svg(small.as_bytes()));
    assert!(!remaining.exceeded());
    // 6 of 7 taken: the third copy does not fit, and from then on nothing does.
    assert!(!remaining.take_svg(small.as_bytes()));
    assert!(remaining.exceeded());
    assert!(!remaining.take_svg(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"));
}

#[test]
fn an_embedded_use_bomb_does_not_fit() {
    let remaining = Remaining::new(MAX_EXPANDED_ELEMENTS);
    assert!(!remaining.take_svg(use_bomb(10, 10).as_bytes()));
    assert!(remaining.exceeded());
}

#[test]
fn what_usvg_cannot_read_as_svg_takes_nothing() {
    let remaining = Remaining::new(1);
    assert!(remaining.take_svg(b"\x89PNG\r\n\x1a\n"));
    assert!(remaining.take_svg(b"GIF89a not xml"));
    assert!(!remaining.exceeded());
    assert!(read_as_svg("image/svg+xml") && read_as_svg("text/plain"));
    assert!(!read_as_svg("image/png") && !read_as_svg("application/xml"));
}
