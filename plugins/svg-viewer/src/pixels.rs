//! The pattern budget of the drawing. resvg draws a pattern not tile by tile: each time a path
//! is filled or stroked with one, it draws one tile into a canvas of its own, at the scale the
//! path is drawn at, and repeats that canvas as a shader (`render_pattern_pixmap`,
//! `resvg-0.49.0/src/path.rs:179-206`). The tile's size in pixels is the cost, and a pattern
//! inside a pattern's tile is drawn again for every tile canvas. The built tree is walked as
//! resvg walks it, and the pixels of every tile canvas are summed before anything is drawn.

use resvg::tiny_skia::Transform;
use resvg::usvg::{self, Group, ImageKind, Node, Paint};

/// The most pixels all tile canvases of one picture may have together: four pictures of the
/// largest size, 64 MiB at four bytes a pixel.
pub const MAX_PATTERN_PIXELS: u64 = 1 << 24;

/// The pixels of every tile canvas resvg draws for `tree` at `transform`, up to `limit + 1`: a
/// result above `limit` means "too many".
#[must_use]
pub fn pattern_pixels(tree: &usvg::Tree, transform: Transform, limit: u64) -> u64 {
    let mut walk = Walk { total: 0, limit };
    // `resvg::render` draws the root's children at the transform (`src/lib.rs:34-50`).
    let _ = walk.nodes(tree.root(), transform);
    walk.total.min(limit.saturating_add(1))
}

struct Walk {
    total: u64,
    limit: u64,
}

/// The budget is spent: the walk stops.
struct Spent;

impl Walk {
    /// The children of `parent`, at `transform` (`render_nodes`, `src/render.rs:10-48`).
    fn nodes(&mut self, parent: &Group, transform: Transform) -> Result<(), Spent> {
        for node in parent.children() {
            match node {
                Node::Group(group) => self.group(group, transform)?,
                Node::Path(path) => {
                    for paint in [
                        path.fill().map(usvg::Fill::paint),
                        path.stroke().map(usvg::Stroke::paint),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        self.paint(paint, transform)?;
                    }
                }
                // A nested SVG is drawn at the transform (`src/image.rs:1-50`).
                Node::Image(image) => {
                    if let ImageKind::SVG(tree) = image.kind() {
                        self.nodes(tree.root(), transform)?;
                    }
                }
                Node::Text(text) => self.group(text.flattened(), transform)?,
            }
        }
        Ok(())
    }

    /// A group: its transform, its mask and the pictures of its filters, then its children
    /// (`render_group`, `src/render.rs:50-110`; `src/mask.rs:6-45`; `apply_image`,
    /// `src/filter/mod.rs:873-896`).
    fn group(&mut self, group: &Group, transform: Transform) -> Result<(), Spent> {
        let transform = transform.pre_concat(group.transform());
        let mut mask = group.mask();
        while let Some(current) = mask {
            self.nodes(current.root(), transform)?;
            mask = current.mask();
        }
        let (sx, sy) = transform.get_scale();
        for filter in group.filters() {
            for primitive in filter.primitives() {
                if let usvg::filter::Kind::Image(image) = primitive.kind() {
                    self.nodes(image.root(), Transform::from_scale(sx, sy))?;
                }
            }
        }
        self.nodes(group, transform)
    }

    /// A pattern's tile canvas and what is drawn into it.
    fn paint(&mut self, paint: &Paint, transform: Transform) -> Result<(), Spent> {
        let Paint::Pattern(pattern) = paint else {
            return Ok(());
        };
        let (sx, sy) = transform.pre_concat(pattern.transform()).get_scale();
        let rect = pattern.rect();
        let width = (rect.width() * sx).round() as u32;
        let height = (rect.height() * sy).round() as u32;
        if width == 0 || height == 0 {
            // No canvas, nothing drawn (`IntSize::from_wh` is `None`).
            return Ok(());
        }
        self.total = self
            .total
            .saturating_add(u64::from(width) * u64::from(height));
        if self.total > self.limit {
            return Err(Spent);
        }
        self.nodes(pattern.root(), Transform::from_scale(sx, sy))
    }
}

#[cfg(test)]
#[path = "pixels_tests.rs"]
mod tests;
