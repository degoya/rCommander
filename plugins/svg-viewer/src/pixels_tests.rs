use std::sync::Arc;

use super::*;
use crate::budget::{Cost, Remaining};
use crate::render::options;

const ANY: u64 = u64::MAX - 1;

/// Two rectangles over (0, 0) to (10, 10), so usvg keeps their group.
const TWO_RECTS: &str = r#"<rect width="10" height="10"/><rect width="5" height="5"/>"#;

fn tree(svg: &str) -> usvg::Tree {
    let options = options(Arc::new(Remaining::new(Cost::LIMIT)));
    usvg::Tree::from_str(svg, &options).expect("the test SVG builds")
}

/// The pattern tiles of `tree` on a 100 x 100 canvas at `transform`, up to `limit + 1`.
fn pattern_pixels(tree: &usvg::Tree, transform: Transform, limit: u64) -> u64 {
    let limit = Pixels {
        patterns: limit,
        layers: ANY,
    };
    canvas_pixels(tree, (100, 100), transform, limit).patterns
}

/// The intermediate canvases of a 100 x 100 document around `body`, drawn at its size.
fn layer_pixels(body: &str) -> u64 {
    let svg =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">{body}</svg>"#);
    let limit = Pixels {
        patterns: ANY,
        layers: ANY,
    };
    canvas_pixels(&tree(&svg), (100, 100), Transform::identity(), limit).layers
}

/// A 100 x 100 document around `body`, with a pattern `inner` of a 5 x 5 tile.
fn document(body: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <pattern id="inner" width="5" height="5" patternUnits="userSpaceOnUse">
            <rect width="1" height="1"/>
        </pattern>{body}</svg>"##
    )
}

#[test]
fn a_pattern_costs_its_tile_at_the_scale_it_is_drawn() {
    let tree = tree(&document(
        r##"<pattern id="p" width="10" height="20" patternUnits="userSpaceOnUse">
            <rect width="5" height="5"/>
        </pattern>
        <rect width="100" height="100" fill="url(#p)"/>"##,
    ));
    assert_eq!(pattern_pixels(&tree, Transform::identity(), ANY), 10 * 20);
    assert_eq!(
        pattern_pixels(&tree, Transform::from_scale(2.0, 2.0), ANY),
        20 * 40
    );
}

#[test]
fn a_pattern_inside_a_pattern_is_drawn_for_every_tile_canvas() {
    // The outer tile (10 x 10) is drawn once; in it four rectangles paint the inner pattern,
    // each with a tile canvas of its own (5 x 5).
    let tree = tree(&document(
        r##"<pattern id="outer" width="10" height="10" patternUnits="userSpaceOnUse">
            <rect width="5" height="5" fill="url(#inner)"/>
            <rect x="5" width="5" height="5" fill="url(#inner)"/>
            <rect y="5" width="5" height="5" fill="url(#inner)"/>
            <rect x="5" y="5" width="5" height="5" fill="url(#inner)"/>
        </pattern>
        <rect width="100" height="100" fill="url(#outer)"/>"##,
    ));
    assert_eq!(
        pattern_pixels(&tree, Transform::identity(), ANY),
        100 + 4 * 25
    );
}

#[test]
fn fill_and_stroke_each_draw_a_tile() {
    let tree = tree(&document(
        r##"<rect width="100" height="100" fill="url(#inner)" stroke="url(#inner)"/>"##,
    ));
    assert_eq!(pattern_pixels(&tree, Transform::identity(), ANY), 2 * 25);
}

#[test]
fn the_walk_stops_beyond_the_limit() {
    let tree = tree(&document(
        r##"<rect width="100" height="100" fill="url(#inner)"/>"##,
    ));
    assert_eq!(pattern_pixels(&tree, Transform::identity(), 10), 11);
    assert_eq!(pattern_pixels(&tree, Transform::identity(), 25), 25);
}

#[test]
fn a_group_drawn_in_place_takes_no_canvas() {
    assert_eq!(layer_pixels(&format!("<g>{TWO_RECTS}</g>")), 0);
    assert_eq!(layer_pixels(TWO_RECTS), 0);
}

#[test]
fn a_group_with_an_opacity_takes_a_canvas_of_its_bounds_and_two_pixels_around() {
    let group = format!(r#"<g opacity="0.5">{TWO_RECTS}</g>"#);
    assert_eq!(layer_pixels(&group), 14 * 14);
    // Nested, each takes its own.
    let nested = format!(r#"<g opacity="0.5">{group}</g>"#);
    assert_eq!(layer_pixels(&nested), 2 * 14 * 14);
    // Side by side, they add up.
    assert_eq!(layer_pixels(&group.repeat(3)), 3 * 14 * 14);
}

#[test]
fn a_mask_and_a_clip_path_take_canvases_of_the_group_s_size() {
    let mask = format!(
        r##"<mask id="m"><rect width="10" height="10" fill="white"/></mask>
        <g mask="url(#m)">{TWO_RECTS}</g>"##
    );
    // The group's canvas, the mask's and its two one-byte masks.
    assert_eq!(layer_pixels(&mask), 3 * 14 * 14);
    let clip = format!(
        r##"<clipPath id="c"><rect width="5" height="5"/></clipPath>
        <g clip-path="url(#c)">{TWO_RECTS}</g>"##
    );
    // usvg cuts the group's canvas to the clip path (`calculate_bounding_boxes`,
    // `usvg-0.49.0/src/tree/mod.rs:1925-1937`): 5 x 5 and the margin, twice.
    assert_eq!(layer_pixels(&clip), 2 * 9 * 9);
}

#[test]
fn a_filter_takes_its_region_for_each_primitive_and_each_input() {
    let filter = format!(
        r##"<filter id="f" filterUnits="userSpaceOnUse" x="0" y="0" width="10" height="10">
            <feFlood flood-color="red"/><feOffset dx="1"/>
        </filter>
        <g filter="url(#f)">{TWO_RECTS}</g>"##
    );
    // The canvas is the region (10 x 10); the flood makes one, the offset one and a copy.
    assert_eq!(layer_pixels(&filter), 100 + 100 + 2 * 100);
}

#[test]
fn an_embedded_svg_takes_a_canvas_of_the_one_it_is_drawn_on() {
    let inner = crate::test_support::data_url(
        "image/svg+xml",
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#,
    );
    let image = format!(r#"<image href="{inner}" width="10" height="10"/>"#);
    assert!(
        layer_pixels(&image) >= 100 * 100,
        "{}",
        layer_pixels(&image)
    );
}

#[test]
fn the_walk_stops_beyond_the_layer_limit_and_beyond_its_nodes() {
    let groups = format!(r#"<g opacity="0.5">{TWO_RECTS}</g>"#).repeat(3);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">{groups}</svg>"#
    );
    let tree = tree(&svg);
    let limit = Pixels {
        patterns: ANY,
        layers: 200,
    };
    let counted = canvas_pixels(&tree, (100, 100), Transform::identity(), limit);
    assert_eq!(counted.layers, 201);
    assert!(counted.exceeds(limit));
    // Three groups of two rectangles are nine nodes: eight are too many.
    let limit = Pixels {
        patterns: 1000,
        layers: 1000,
    };
    let over = Pixels {
        patterns: 1001,
        layers: 1001,
    };
    assert_eq!(
        walk(&tree, (100, 100), Transform::identity(), limit, 8),
        over
    );
    assert!(!walk(&tree, (100, 100), Transform::identity(), limit, 9).exceeds(limit));
}
