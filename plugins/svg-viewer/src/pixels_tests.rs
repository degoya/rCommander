use std::sync::Arc;

use super::*;
use crate::budget::{MAX_EXPANDED_ELEMENTS, Remaining};
use crate::render::options;

const ANY: u64 = u64::MAX - 1;

fn tree(svg: &str) -> usvg::Tree {
    let options = options(Arc::new(Remaining::new(MAX_EXPANDED_ELEMENTS)));
    usvg::Tree::from_str(svg, &options).expect("the test SVG builds")
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
