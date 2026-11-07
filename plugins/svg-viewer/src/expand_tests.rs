use roxmltree::Document;

use super::*;
use crate::budget::{MAX_ARC_CURVES, MAX_EXPANDED_ELEMENTS};

const MAX: u64 = MAX_EXPANDED_ELEMENTS;

/// What `svg` costs, each part up to its `limit + 1`.
fn cost(svg: &str, limit: Cost) -> Cost {
    let doc = Document::parse(svg).expect("the test SVG parses");
    count(&doc, limit)
}

/// The elements of `svg`, its curves within their budget.
fn counted(svg: &str, limit: u64) -> u64 {
    let limit = Cost {
        elements: limit,
        curves: MAX_ARC_CURVES,
    };
    cost(svg, limit).elements
}

/// The curves of `svg`, unlimited but for an arc beyond its own limit.
fn curves(svg: &str) -> u64 {
    let limit = Cost {
        elements: MAX,
        curves: u64::MAX - 1,
    };
    cost(svg, limit).curves
}

/// A document around `body`.
fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#)
}

/// The `points` of a polyline of `count` vertices.
fn points(count: usize) -> String {
    (0..count)
        .map(|x| format!("{x},0"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_few_markers_and_a_pattern_cost_little() {
    let svg = svg(r##"<defs>
            <marker id="arrow"><path d="M0 0 L10 5 L0 10 Z"/></marker>
            <pattern id="dots" width="10" height="10"><circle r="2"/></pattern>
        </defs>
        <line x1="0" y1="0" x2="100" y2="0" marker-end="url(#arrow)"/>
        <polyline points="0,0 10,10 20,0" marker-start="url(#arrow)" marker-mid="url(#arrow)"/>
        <rect width="100" height="100" fill="url(#dots)"/>"##);
    // Nine elements where they stand; the arrow (2 elements) drawn once on the line and 1 + 3
    // times on the polyline; the dots (2 elements) copied once for the rectangle.
    assert_eq!(counted(&svg, 100), 9 + 5 * 2 + 2);
}

#[test]
fn a_marker_counts_once_per_vertex() {
    // 2 000 vertices, each drawing a marker of 1 000 rectangles: two million elements.
    let rects = "<rect/>".repeat(1000);
    let svg = svg(&format!(
        r##"<marker id="m">{rects}</marker><polyline points="{}" marker-mid="url(#m)"/>"##,
        points(2000)
    ));
    assert_eq!(counted(&svg, MAX), MAX + 1);
}

#[test]
fn markers_inside_markers_multiply() {
    // Four markers, each drawn on the 100 vertices of a polyline inside the next: 100^4.
    let mut body = String::from(r#"<marker id="m0"><rect/></marker>"#);
    for level in 1..=4 {
        body.push_str(&format!(
            r##"<marker id="m{level}"><polyline points="{}" marker-mid="url(#m{})"/></marker>"##,
            points(100),
            level - 1
        ));
    }
    body.push_str(&format!(
        r##"<polyline points="{}" marker-mid="url(#m4)"/>"##,
        points(100)
    ));
    assert_eq!(counted(&svg(&body), MAX), MAX + 1);
}

#[test]
fn a_marker_inherited_set_in_css_or_reached_through_a_use_counts_too() {
    let marker = format!(r#"<marker id="m">{}</marker>"#, "<rect/>".repeat(1000));
    let line = format!(r#"points="{}""#, points(2000));
    let documents = [
        // On a group, inherited by the polyline.
        format!(r##"{marker}<g marker-mid="url(#m)"><polyline {line}/></g>"##),
        // On a group around a `<use>`: the copy inherits from where the `<use>` stands.
        format!(
            r##"<defs><polyline id="p" {line}/></defs>{marker}<g marker-mid="url(#m)"><use href="#p"/></g>"##
        ),
        // In a style sheet, and in a `style` attribute.
        format!(r##"<style>polyline {{ marker-mid: url(#m) }}</style>{marker}<polyline {line}/>"##),
        format!(r##"{marker}<polyline {line} style="marker: url(#m)"/>"##),
    ];
    for body in documents {
        assert_eq!(counted(&svg(&body), MAX), MAX + 1, "{}", &body[..80]);
    }
}

#[test]
fn an_arc_of_a_huge_radius_counts_its_curves_as_vertices() {
    // Some 75 000 curves, each a vertex drawing a marker of 21 elements.
    let svg = svg(&format!(
        r##"<marker id="m">{}</marker><path d="M0 0 A1e30 1e30 0 0 1 2e30 0" marker-mid="url(#m)"/>"##,
        "<rect/>".repeat(20)
    ));
    assert_eq!(counted(&svg, MAX), MAX + 1);
}

#[test]
fn a_marker_inside_itself_is_skipped_and_a_ring_of_two_is_refused() {
    let own = svg(
        r##"<marker id="m"><line marker-end="url(#m)"/></marker><line marker-end="url(#m)"/>"##,
    );
    // The svg, the marker, its line (drawing the marker where it stands, as all of `defs`
    // counts) and the other line: the marker drawn twice as 2 elements, its own link skipped.
    assert_eq!(counted(&own, 100), 4 + 2 * 2);
    let ring = svg(r##"<marker id="a"><line marker-end="url(#b)"/></marker>
        <marker id="b"><line marker-end="url(#a)"/></marker><line marker-end="url(#a)"/>"##);
    assert_eq!(counted(&ring, 100), 101);
}

#[test]
fn a_pattern_counts_once_per_painted_shape() {
    // 1 100 rectangles, each taking a copy of 1 001 elements.
    let svg = svg(&format!(
        r#"<pattern id="p" width="1" height="1">{}</pattern>{}"#,
        "<rect/>".repeat(1000),
        r##"<rect fill="url(#p)"/>"##.repeat(1100)
    ));
    assert_eq!(counted(&svg, MAX), MAX + 1);
}

#[test]
fn patterns_inside_patterns_multiply() {
    // Each level paints 100 rectangles with the one before: 100^4 copies.
    let mut body = String::from(r#"<pattern id="p0"><rect/></pattern>"#);
    for level in 1..=4 {
        body.push_str(&format!(
            r#"<pattern id="p{level}">{}</pattern>"#,
            format!(r##"<rect fill="url(#p{})"/>"##, level - 1).repeat(100)
        ));
    }
    body.push_str(r##"<rect fill="url(#p4)"/>"##);
    assert_eq!(counted(&svg(&body), MAX), MAX + 1);
}

#[test]
fn a_pattern_takes_the_content_its_href_names() {
    let svg = svg(&format!(
        r##"<pattern id="q">{}</pattern><pattern id="p" href="#q"/>{}"##,
        "<rect/>".repeat(1000),
        r##"<rect fill="url(#p)"/>"##.repeat(1100)
    ));
    assert_eq!(counted(&svg, MAX), MAX + 1);
}

#[test]
fn a_pattern_painting_itself_is_cleared_and_other_rings_are_refused() {
    // usvg clears the link of a pattern's own content to the pattern: the copy is 2 elements.
    let own = svg(r##"<pattern id="p"><rect fill="url(#p)"/></pattern><rect fill="url(#p)"/>"##);
    assert_eq!(counted(&own, 100), 4 + 2 * 2);
    // A link inherited from the pattern itself, and a ring of three, usvg follows forever.
    let inherited =
        svg(r##"<pattern id="p" fill="url(#p)"><rect/></pattern><rect fill="url(#p)"/>"##);
    let ring = svg(r##"<pattern id="a"><rect fill="url(#b)"/></pattern>
        <pattern id="b"><rect fill="url(#c)"/></pattern>
        <pattern id="c"><rect fill="url(#a)"/></pattern><rect fill="url(#a)"/>"##);
    assert_eq!(counted(&inherited, 100), 101);
    assert_eq!(counted(&ring, 100), 101);
}

#[test]
fn painted_text_takes_a_copy_per_character_and_line() {
    let svg = svg(r##"<pattern id="p"><rect/></pattern><text fill="url(#p)">abc</text>"##);
    // The svg, the pattern and its rect; the text with (3 + 1) * 4 copies of 2 elements.
    assert_eq!(counted(&svg, 100), 3 + 1 + 16 * 2);
}

#[test]
fn curves_count_as_often_as_usvg_builds_their_shape() {
    // Half a circle of radius 1 is two curves: in place, and twice more through `use`.
    let document =
        svg(r##"<path id="p" d="M0 0 A1 1 0 0 1 2 0"/><use href="#p"/><use href="#p"/>"##);
    assert_eq!(curves(&document), 3 * 2);
    // A circle (four curves) in a marker drawn at both ends of a line, and where it stands.
    let document = svg(r##"<marker id="m"><circle r="10"/></marker>
        <line x1="0" y1="0" x2="10" y2="0" marker-start="url(#m)" marker-end="url(#m)"/>"##);
    assert_eq!(curves(&document), 3 * 4);
}

#[test]
fn a_text_path_counts_the_curves_of_the_shape_it_follows() {
    let document = svg(r##"<path id="p" d="M0 0 A1 1 0 0 1 2 0"/>
        <text><textPath href="#p">ab</textPath><textPath xlink:href="#p"
            xmlns:xlink="http://www.w3.org/1999/xlink">c</textPath></text>"##);
    assert_eq!(curves(&document), 3 * 2);
}

#[test]
fn an_arc_beyond_its_own_limit_is_refused_at_once() {
    let document = svg(r#"<path d="M0 0 A1e30 1e30 0 0 1 2e30 0"/>"#);
    let limit = Cost {
        elements: MAX,
        curves: MAX_ARC_CURVES,
    };
    let counted = cost(&document, limit);
    assert!(counted.exceeds(limit));
    assert_eq!(counted.curves, MAX_ARC_CURVES + 1);
}

#[test]
fn arcs_within_their_limit_add_up_beyond_the_budget_through_use() {
    // Ten arcs of some 48 curves each, in five levels of ten `<use>`: 11 111 copies of them,
    // over five million curves, from some 136 000 elements.
    let mut body = format!(
        r#"<g id="l0">{}</g>"#,
        r#"<path d="M0 0 A1e9 1e9 0 1 1 1 0"/>"#.repeat(10)
    );
    for level in 1..5 {
        let uses = format!(r##"<use href="#l{}"/>"##, level - 1).repeat(10);
        body.push_str(&format!(r#"<g id="l{level}">{uses}</g>"#));
    }
    let document = svg(&body);
    let unlimited = Cost {
        elements: MAX,
        curves: u64::MAX - 1,
    };
    let counted = cost(&document, unlimited);
    assert!(counted.elements < MAX, "{counted:?}");
    assert!(counted.curves > MAX_ARC_CURVES, "{counted:?}");
    let limit = Cost {
        elements: MAX,
        curves: MAX_ARC_CURVES,
    };
    assert_eq!(cost(&document, limit).curves, MAX_ARC_CURVES + 1);
}
