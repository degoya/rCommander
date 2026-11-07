use roxmltree::Document;

use super::*;

/// What usvg makes of the one element `element`.
fn shape_of(element: &str) -> Shape {
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{element}</svg>"#);
    let doc = Document::parse(&svg).expect("the test SVG parses");
    shape(
        doc.root_element()
            .first_element_child()
            .expect("the element"),
    )
}

/// The segments of the one element `element`.
fn of(element: &str) -> u64 {
    shape_of(element).segments
}

/// The curves of the one element `element`.
fn curves_of(element: &str) -> u64 {
    shape_of(element).curves
}

/// The cubic curves kurbo itself makes of one arc from (0, 0), as svgtypes asks it.
fn kurbo_makes(rx: f64, ry: f64, large_arc: bool, to: (f64, f64)) -> u64 {
    let arc = kurbo::SvgArc {
        from: kurbo::Point::new(0.0, 0.0),
        to: kurbo::Point::new(to.0, to.1),
        radii: kurbo::Vec2::new(rx, ry),
        x_rotation: 0.0,
        large_arc,
        sweep: true,
    };
    kurbo::Arc::from_svg_arc(&arc)
        .map(|arc| arc.append_iter(TOLERANCE).count() as u64)
        .unwrap_or(0)
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

#[test]
fn an_arc_counts_its_curves_exactly_as_kurbo_makes_them() {
    for (rx, ry, large_arc, to) in [
        (1.0, 1.0, false, (2.0, 0.0)),
        (100.0, 50.0, true, (10.0, 10.0)),
        (1e6, 1e6, false, (5.0, 0.0)),
        (1e6, 1e6, true, (5.0, 0.0)),
        (1e9, 1e9, true, (1.0, 0.0)),
    ] {
        let flag = u8::from(large_arc);
        let d = format!("M0 0 A{rx} {ry} 0 {flag} 1 {} {}", to.0, to.1);
        let expected = kurbo_makes(rx, ry, large_arc, to);
        assert!(expected <= MAX_CURVES_PER_ARC, "{d}: {expected}");
        assert_eq!(curves_of(&format!(r#"<path d="{d}"/>"#)), expected, "{d}");
    }
    // Two arcs add up; lines and curves make none.
    assert_eq!(
        curves_of(r#"<path d="M0 0 A1 1 0 0 1 2 0 A1 1 0 0 1 4 0 L5 5 C1 1 2 2 3 3 Z"/>"#),
        2 * kurbo_makes(1.0, 1.0, false, (2.0, 0.0))
    );
}

#[test]
fn an_arc_beyond_its_limit_counts_beyond_any_budget() {
    // Half an ellipse of radius 10^30: some 75 000 curves, which svgtypes would buffer and take
    // out from the front one by one.
    let bomb = r#"<path d="M0 0 A1e30 1e30 0 0 1 2e30 0"/>"#;
    assert!(kurbo_makes(1e30, 1e30, false, (2e30, 0.0)) > 50_000);
    assert_eq!(curves_of(bomb), u64::MAX);
    // Between two close points the same radius bends too little for f64 to see: the large arc
    // there is a sweep of nothing, and kurbo makes as little as it counts.
    for large_arc in [false, true] {
        let flag = u8::from(large_arc);
        let d = format!(r#"<path d="M0 0 A1e30 1e30 0 {flag} 1 1 0"/>"#);
        let made = kurbo_makes(1e30, 1e30, large_arc, (1.0, 0.0));
        assert!(made <= 1, "{made}");
        assert_eq!(curves_of(&d), made);
    }
}

#[test]
fn circles_ellipses_and_rounded_rects_count_the_curves_of_four_quarters() {
    assert_eq!(curves_of(r#"<circle r="10"/>"#), 4);
    assert_eq!(curves_of(r#"<ellipse rx="10" ry="5"/>"#), 4);
    assert_eq!(curves_of(r#"<rect width="10" height="10" rx="2"/>"#), 4);
    assert_eq!(curves_of(r#"<rect width="10" height="10"/>"#), 0);
    assert_eq!(curves_of(r#"<line x1="0" y1="0" x2="1" y2="1"/>"#), 0);
    // usvg clamps a corner to half the side.
    assert_eq!(curves_of(r#"<rect width="10" height="10" rx="1e30"/>"#), 4);
    // A huge radius makes a great many, without a limit of its own: tiny-skia takes them as
    // they come.
    let huge = curves_of(r#"<circle r="1e30"/>"#);
    assert!(huge > 100_000 && huge < u64::MAX, "{huge}");
    // A radius in `em` counts as the smallest for the curves, the largest for the segments.
    assert_eq!(curves_of(r#"<circle r="1em"/>"#), 4);
}
