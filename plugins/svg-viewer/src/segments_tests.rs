use roxmltree::Document;

use super::*;

/// The segments of the one element `element`.
fn of(element: &str) -> u64 {
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{element}</svg>"#);
    let doc = Document::parse(&svg).expect("the test SVG parses");
    segments(
        doc.root_element()
            .first_element_child()
            .expect("the element"),
    )
}

#[test]
fn lines_and_points_count_their_vertices() {
    assert_eq!(of(r#"<line x1="0" y1="0" x2="1" y2="1"/>"#), 2);
    assert_eq!(of(r#"<polyline points="0,0 1,1 2,0"/>"#), 3);
    assert_eq!(of(r#"<polygon points="0,0 1,1 2,0"/>"#), 4);
    assert_eq!(of(r#"<polyline points="0,0"/>"#), 0);
    assert_eq!(of("<g/>"), 0);
}

#[test]
fn a_path_counts_each_command_and_the_implicit_move_after_a_close() {
    // M, L, L, Z, an implicit M before the `l`, the `l`, H and V.
    assert_eq!(of(r#"<path d="M0 0 L1 0 1 1 Z l1 1 H5 V5"/>"#), 8);
    // Parsing stops at the first error, as usvg's does.
    assert_eq!(of(r#"<path d="M0 0 L1 0 X 2 2"/>"#), 2);
}

#[test]
fn an_arc_counts_the_curves_kurbo_cuts_it_into() {
    // Half a circle of radius 1: two curves, one more for the rounding, after the move.
    assert_eq!(of(r#"<path d="M0 0 A1 1 0 0 1 2 0"/>"#), 1 + 3);
    // Half a circle of radius 10^30: some 75 000 curves.
    let huge = of(r#"<path d="M0 0 A1e30 1e30 0 0 1 2e30 0"/>"#);
    assert!(huge > 50_000, "{huge}");
}

#[test]
fn circles_ellipses_and_rounded_rects_count_four_quarter_arcs() {
    // A small radius: one curve a quarter, one more for the rounding.
    assert_eq!(of(r#"<circle r="10"/>"#), 2 + 4 * 2);
    assert_eq!(of(r#"<ellipse rx="10" ry="5"/>"#), 2 + 4 * 2);
    assert_eq!(of(r#"<rect width="10" height="10"/>"#), 5);
    assert_eq!(of(r#"<rect width="10" height="10" rx="2"/>"#), 6 + 4 * 2);
    // A radius in `em` may be of any size: the largest usvg can hold counts.
    assert!(of(r#"<circle r="1em"/>"#) > 4 * 100_000);
}
