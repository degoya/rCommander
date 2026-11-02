//! How many segments usvg 0.49 makes of a shape: the vertices its markers are drawn on, one
//! marker at the start, one at the end and one at each vertex between
//! (`draw_markers`, `src/parser/marker.rs:216-262`). An upper bound, from the attributes alone:
//! a `d` is walked as `svgtypes`' `SimplifyingPathParser` walks it (`svgtypes-0.16.1/src/path.rs:
//! 751-1035`), and every elliptical arc counts the cubic curves kurbo cuts it into
//! (`Arc::append_iter`, `kurbo-0.13.1/src/arc.rs:77-85`) - a huge radius makes a great many.

use std::f64::consts::PI;

use roxmltree::Node;
use svgtypes::{LengthUnit, PathParser, PathSegment, PointsParser};

use crate::refs::SVG_NS;

/// The tolerance usvg and svgtypes hand kurbo (`arc.to_cubic_beziers(0.1, ...)`).
const TOLERANCE: f64 = 0.1;

/// At most how many segments usvg makes of `node`; 0 for an element that is no shape.
#[must_use]
pub fn segments(node: Node) -> u64 {
    match node.tag_name().name() {
        "path" => values(node, "d").map(path).max().unwrap_or(0),
        "line" => 2,
        "polyline" => values(node, "points").map(points).max().unwrap_or(0),
        // `points_to_path` and a `close` (`src/parser/shapes.rs:182-223`).
        "polygon" => values(node, "points")
            .map(|value| points(value).saturating_add(1))
            .max()
            .unwrap_or(0),
        // A move, four lines, four corner arcs and a close (`convert_rect`, `shapes.rs:69-127`).
        "rect" => {
            let radius = radius(node, &["rx", "ry"]);
            if radius > 0.0 {
                quarter(radius).saturating_mul(4).saturating_add(6)
            } else {
                5
            }
        }
        // A move, four quarter arcs and a close (`ellipse_to_path`, `shapes.rs:265-274`).
        "circle" => quarter(radius(node, &["r"]))
            .saturating_mul(4)
            .saturating_add(2),
        "ellipse" => quarter(radius(node, &["rx", "ry"]))
            .saturating_mul(4)
            .saturating_add(2),
        _ => 0,
    }
}

/// The values of the attribute `name` usvg may read (`src/parser/svgtree/parse.rs:235-238`).
fn values<'a>(node: Node<'a, '_>, name: &'a str) -> impl Iterator<Item = &'a str> {
    node.attributes()
        .filter(move |attribute| {
            attribute.name() == name && matches!(attribute.namespace(), None | Some(SVG_NS))
        })
        .map(|attribute| attribute.value())
}

/// The points of a `points` value, as `PointsParser` reads them; fewer than two is no shape.
fn points(value: &str) -> u64 {
    let count = PointsParser::from(value).count() as u64;
    if count < 2 { 0 } else { count }
}

/// The largest radius among `names` in user units; a unit whose size depends on the font or the
/// viewport counts as the largest radius usvg can hold (`f32`).
fn radius(node: Node, names: &[&str]) -> f64 {
    let largest = f64::from(f32::MAX);
    names
        .iter()
        .flat_map(|name| values(node, name))
        .map(|value| match value.parse::<svgtypes::Length>() {
            Ok(length) => {
                let scale = match length.unit {
                    LengthUnit::None | LengthUnit::Px => 1.0,
                    LengthUnit::In => 96.0,
                    LengthUnit::Cm => 96.0 / 2.54,
                    LengthUnit::Mm => 96.0 / 25.4,
                    LengthUnit::Pt => 4.0 / 3.0,
                    LengthUnit::Pc => 16.0,
                    LengthUnit::Em | LengthUnit::Ex | LengthUnit::Percent => return largest,
                };
                let radius = (length.number * scale).abs();
                if radius.is_nan() {
                    largest
                } else {
                    radius.min(largest)
                }
            }
            Err(_) => 0.0,
        })
        .fold(0.0, f64::max)
}

/// The cubic curves of a quarter of an ellipse of `radius`.
fn quarter(radius: f64) -> u64 {
    cubics(radius, PI / 2.0)
}

/// The cubic curves kurbo cuts an arc of `radius` and `sweep` into (`kurbo-0.13.1/src/arc.rs:
/// 79-85`), one more for the rounding of the point the next segment starts from.
fn cubics(radius: f64, sweep: f64) -> u64 {
    let n_err = (1.1163 * (radius / TOLERANCE))
        .powf(1.0 / 6.0)
        .max(3.999_999);
    let n = (n_err * sweep.abs() * (1.0 / (2.0 * PI))).ceil();
    if n.is_nan() {
        1
    } else if n >= u64::MAX as f64 {
        u64::MAX
    } else {
        (n as u64).saturating_add(1)
    }
}

/// The segments of a `d` value: one per command, an implicit move after a close that is not
/// followed by a move or close, and an arc's cubic curves. Parsing stops at the first error,
/// as usvg's does (`convert_path`, `src/parser/shapes.rs:26-36`).
fn path(value: &str) -> u64 {
    let mut count = 0u64;
    let (mut x, mut y) = (0.0, 0.0);
    let (mut start_x, mut start_y) = (0.0, 0.0);
    let mut closed = false;
    for segment in PathParser::from(value) {
        let Ok(segment) = segment else {
            break;
        };
        let close = matches!(segment, PathSegment::ClosePath { .. });
        if closed && !close && !matches!(segment, PathSegment::MoveTo { .. }) {
            count = count.saturating_add(1);
            (x, y) = (start_x, start_y);
        }
        let added = match segment {
            PathSegment::MoveTo {
                abs,
                x: to_x,
                y: to_y,
            } => {
                let (base_x, base_y) = if closed { (start_x, start_y) } else { (x, y) };
                (x, y) = relative(abs, (base_x, base_y), (to_x, to_y));
                (start_x, start_y) = (x, y);
                1
            }
            PathSegment::LineTo {
                abs,
                x: to_x,
                y: to_y,
            }
            | PathSegment::SmoothQuadratic {
                abs,
                x: to_x,
                y: to_y,
            }
            | PathSegment::Quadratic {
                abs,
                x: to_x,
                y: to_y,
                ..
            }
            | PathSegment::CurveTo {
                abs,
                x: to_x,
                y: to_y,
                ..
            }
            | PathSegment::SmoothCurveTo {
                abs,
                x: to_x,
                y: to_y,
                ..
            } => {
                (x, y) = relative(abs, (x, y), (to_x, to_y));
                1
            }
            PathSegment::HorizontalLineTo { abs, x: to_x } => {
                x = if abs { to_x } else { x + to_x };
                1
            }
            PathSegment::VerticalLineTo { abs, y: to_y } => {
                y = if abs { to_y } else { y + to_y };
                1
            }
            PathSegment::EllipticalArc {
                abs,
                rx,
                ry,
                x_axis_rotation,
                large_arc,
                sweep,
                x: to_x,
                y: to_y,
            } => {
                let to = relative(abs, (x, y), (to_x, to_y));
                let arc = kurbo::SvgArc {
                    from: kurbo::Point::new(x, y),
                    to: kurbo::Point::new(to.0, to.1),
                    radii: kurbo::Vec2::new(rx, ry),
                    x_rotation: x_axis_rotation.to_radians(),
                    large_arc,
                    sweep,
                };
                (x, y) = to;
                match kurbo::Arc::from_svg_arc(&arc) {
                    Some(arc) => cubics(arc.radii.x.max(arc.radii.y), arc.sweep_angle),
                    None => 1,
                }
            }
            PathSegment::ClosePath { .. } => {
                (x, y) = (start_x, start_y);
                1
            }
        };
        count = count.saturating_add(added);
        closed = close;
    }
    count
}

fn relative(abs: bool, from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    if abs {
        to
    } else {
        (from.0 + to.0, from.1 + to.1)
    }
}

#[cfg(test)]
#[path = "segments_tests.rs"]
mod tests;
