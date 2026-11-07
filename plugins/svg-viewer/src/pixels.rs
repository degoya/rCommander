//! The pixel budgets of the drawing: the canvases resvg 0.49 allocates besides the picture's own,
//! summed before anything is drawn, as resvg walks the built tree.
//!
//! - **Pattern tiles** (RC-100-77). resvg draws a pattern not tile by tile: each time a path is
//!   filled or stroked with one, it draws one tile into a canvas of its own, at the scale the
//!   path is drawn at, and repeats that canvas as a shader (`render_pattern_pixmap`,
//!   `resvg-0.49.0/src/path.rs:179-206`). A pattern inside a tile is drawn again for every tile
//!   canvas. At most [`MAX_PATTERN_PIXELS`] together.
//! - **Intermediate canvases** (RC-100-78). A group with an opacity below 1, a clip path, a mask,
//!   a filter or a blend mode is drawn into a canvas the size of its layer bounding box
//!   (`layers.rs`), nested ones each into their own. On that canvas a clip path takes one more
//!   canvas, and one more for each clipped group inside it (`src/clip.rs`); a mask one canvas
//!   and two one-byte masks, counted as one more (`src/mask.rs`); a filter a copy of the source
//!   where its region is smaller, and each primitive its result and a copy of each input over
//!   the region (`apply_inner`, `src/filter/mod.rs:357-528`). An embedded SVG is drawn into a
//!   canvas of the one it is drawn on (`render_vector`, `src/image.rs:37-54`). At most
//!   [`MAX_LAYER_PIXELS`] together: a budget of their own, since a filter over the whole
//!   picture takes a dozen canvases of its size where a pattern takes one small tile.
//!
//! usvg builds a mask, a clip path or a filter once and resvg draws it again for every group it
//! applies to, so the walk visits at most [`MAX_DRAWN_NODES`] nodes.

use resvg::tiny_skia::{IntRect, Transform};
use resvg::usvg::{self, Group, ImageKind, Node, Paint};

use crate::budget::MAX_EXPANDED_ELEMENTS;
use crate::layers;

/// The most pixels all tile canvases of one picture may have together: four pictures of the
/// largest size, 64 MiB at four bytes a pixel.
pub const MAX_PATTERN_PIXELS: u64 = 1 << 24;

/// The most pixels all intermediate canvases of one picture may have together: 256 MiB at four
/// bytes a pixel, the plugin's whole memory (`manifest.toml`). A drop shadow over the whole
/// largest picture (2048 x 2048) takes some 13 canvases of its size, 55 M pixels.
pub const MAX_LAYER_PIXELS: u64 = 1 << 26;

/// The most nodes resvg may draw, masks, clip paths and filter pictures as often as they apply:
/// four times the element budget.
pub const MAX_DRAWN_NODES: u64 = 4 * MAX_EXPANDED_ELEMENTS;

/// The canvases of a drawing, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pixels {
    /// The pattern tiles.
    pub patterns: u64,
    /// The intermediate canvases.
    pub layers: u64,
}

impl Pixels {
    /// The budget of one picture.
    pub const LIMIT: Self = Self {
        patterns: MAX_PATTERN_PIXELS,
        layers: MAX_LAYER_PIXELS,
    };

    /// Whether either part is beyond `limit`.
    #[must_use]
    pub fn exceeds(self, limit: Self) -> bool {
        self.patterns > limit.patterns || self.layers > limit.layers
    }
}

/// The pixels of every tile canvas and every intermediate canvas resvg draws for `tree` onto a
/// canvas of `size` at `transform`, each up to its `limit + 1`: a result that exceeds `limit`
/// means "too many", and so does a drawing beyond [`MAX_DRAWN_NODES`] nodes.
#[must_use]
pub fn canvas_pixels(
    tree: &usvg::Tree,
    size: (u32, u32),
    transform: Transform,
    limit: Pixels,
) -> Pixels {
    walk(tree, size, transform, limit, MAX_DRAWN_NODES)
}

fn walk(
    tree: &usvg::Tree,
    size: (u32, u32),
    transform: Transform,
    limit: Pixels,
    max_nodes: u64,
) -> Pixels {
    let over = Pixels {
        patterns: limit.patterns.saturating_add(1),
        layers: limit.layers.saturating_add(1),
    };
    let Some(max_bbox) = layers::max_bbox(size) else {
        return Pixels::default();
    };
    let mut state = Walk {
        total: Pixels::default(),
        limit,
        visited: 0,
        max_nodes,
    };
    // `resvg::render` draws the root's children at the transform (`src/lib.rs:34-43`).
    let frame = Frame {
        transform,
        max_bbox,
        canvas: size,
    };
    if state.nodes(tree.root(), frame).is_err() && state.visited > max_nodes {
        return over;
    }
    Pixels {
        patterns: state.total.patterns.min(over.patterns),
        layers: state.total.layers.min(over.layers),
    }
}

/// Where a node is drawn: at what transform, with what largest layer, onto a canvas of what size.
#[derive(Clone, Copy, Debug)]
struct Frame {
    transform: Transform,
    max_bbox: IntRect,
    canvas: (u32, u32),
}

struct Walk {
    total: Pixels,
    limit: Pixels,
    visited: u64,
    max_nodes: u64,
}

/// A budget is spent: the walk stops.
struct Spent;

fn area(size: (u32, u32)) -> u64 {
    u64::from(size.0) * u64::from(size.1)
}

impl Walk {
    fn visit(&mut self) -> Result<(), Spent> {
        self.visited += 1;
        if self.visited > self.max_nodes {
            return Err(Spent);
        }
        Ok(())
    }

    fn layer(&mut self, pixels: u64) -> Result<(), Spent> {
        self.total.layers = self.total.layers.saturating_add(pixels);
        if self.total.layers > self.limit.layers {
            return Err(Spent);
        }
        Ok(())
    }

    /// The children of `parent` (`render_nodes`, `src/render.rs:10-47`).
    fn nodes(&mut self, parent: &Group, frame: Frame) -> Result<(), Spent> {
        for node in parent.children() {
            self.visit()?;
            match node {
                Node::Group(group) => self.group(group, frame)?,
                Node::Path(path) => {
                    for paint in [
                        path.fill().map(usvg::Fill::paint),
                        path.stroke().map(usvg::Stroke::paint),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        self.paint(paint, frame)?;
                    }
                }
                Node::Image(image) => {
                    if let ImageKind::SVG(tree) = image.kind() {
                        // A canvas of the one drawn on, the nested picture drawn into it as a
                        // picture of its own.
                        self.layer(area(frame.canvas))?;
                        if let Some(max_bbox) = layers::max_bbox(frame.canvas) {
                            self.nodes(tree.root(), Frame { max_bbox, ..frame })?;
                        }
                    }
                }
                Node::Text(text) => self.group(text.flattened(), frame)?,
            }
        }
        Ok(())
    }

    /// A group: drawn in place, or into a canvas of its own with its filters, clip path and mask
    /// (`render_group`, `render_layer`, `src/render.rs:49-153`).
    fn group(&mut self, group: &Group, frame: Frame) -> Result<(), Spent> {
        let transform = frame.transform.pre_concat(group.transform());
        if !group.should_isolate() {
            return self.nodes(group, Frame { transform, ..frame });
        }
        let Some(layer) = layers::layer(group, transform, frame.max_bbox) else {
            return Ok(());
        };
        let canvas = (layer.rect.width(), layer.rect.height());
        self.layer(area(canvas))?;
        let inner = Frame {
            transform: layer.transform,
            max_bbox: layer.max_bbox,
            canvas,
        };
        self.nodes(group, inner)?;
        for filter in group.filters() {
            self.filter(filter, inner)?;
        }
        if let Some(clip) = group.clip_path() {
            self.clip(clip, inner)?;
        }
        if let Some(mask) = group.mask() {
            self.mask(mask, inner)?;
        }
        Ok(())
    }

    /// A filter over the canvas of `frame`, and the content of its `feImage` primitives.
    fn filter(&mut self, filter: &usvg::filter::Filter, frame: Frame) -> Result<(), Spent> {
        let Some((region, transform)) =
            layers::filter_region(filter, frame.transform, frame.canvas)
        else {
            return Ok(());
        };
        let pixels = area((region.width(), region.height()));
        if (region.x(), region.y(), region.width(), region.height())
            != (0, 0, frame.canvas.0, frame.canvas.1)
        {
            // The source cropped to the region.
            self.layer(pixels)?;
        }
        for primitive in filter.primitives() {
            self.layer(pixels.saturating_mul(1 + layers::inputs(primitive.kind())))?;
            if let usvg::filter::Kind::Image(image) = primitive.kind()
                && let Some((transform, max_bbox)) = layers::image(primitive, region, transform)
            {
                let frame = Frame {
                    transform,
                    max_bbox,
                    canvas: (region.width(), region.height()),
                };
                self.nodes(image.root(), frame)?;
            }
        }
        Ok(())
    }

    /// A clip path over the canvas of `frame` (`apply`, `src/clip.rs:6-28`).
    fn clip(&mut self, clip: &usvg::ClipPath, frame: Frame) -> Result<(), Spent> {
        self.layer(area(frame.canvas))?;
        let transform = frame.transform.pre_concat(clip.transform());
        self.clip_children(clip.root(), Frame { transform, ..frame })?;
        match clip.clip_path() {
            Some(clip) => self.clip(clip, frame),
            None => Ok(()),
        }
    }

    /// What a clip path fills (`draw_children`, `clip_group`, `src/clip.rs:30-98`): a clipped
    /// group inside it takes one more canvas.
    fn clip_children(&mut self, parent: &Group, frame: Frame) -> Result<(), Spent> {
        for child in parent.children() {
            self.visit()?;
            match child {
                Node::Path(path) => {
                    if let Some(fill) = path.fill() {
                        self.paint(fill.paint(), frame)?;
                    }
                }
                Node::Text(text) => self.clip_children(text.flattened(), frame)?,
                Node::Group(group) => {
                    let transform = frame.transform.pre_concat(group.transform());
                    let inner = Frame { transform, ..frame };
                    if let Some(clip) = group.clip_path() {
                        self.layer(area(frame.canvas))?;
                        self.clip_children(group, inner)?;
                        self.clip(clip, inner)?;
                    } else {
                        self.clip_children(group, inner)?;
                    }
                }
                Node::Image(_) => {}
            }
        }
        Ok(())
    }

    /// A mask over the canvas of `frame`, and the mask of the mask (`apply`, `src/mask.rs`).
    fn mask(&mut self, mask: &usvg::Mask, frame: Frame) -> Result<(), Spent> {
        if mask.root().children().is_empty() {
            return Ok(());
        }
        self.layer(area(frame.canvas).saturating_mul(2))?;
        self.nodes(mask.root(), frame)?;
        match mask.mask() {
            Some(mask) => self.mask(mask, frame),
            None => Ok(()),
        }
    }

    /// A pattern's tile canvas and what is drawn into it.
    fn paint(&mut self, paint: &Paint, frame: Frame) -> Result<(), Spent> {
        let Paint::Pattern(pattern) = paint else {
            return Ok(());
        };
        let (sx, sy) = frame.transform.pre_concat(pattern.transform()).get_scale();
        let rect = pattern.rect();
        let width = (rect.width() * sx).round() as u32;
        let height = (rect.height() * sy).round() as u32;
        if width == 0 || height == 0 {
            // No canvas, nothing drawn (`IntSize::from_wh` is `None`).
            return Ok(());
        }
        self.total.patterns = self
            .total
            .patterns
            .saturating_add(u64::from(width) * u64::from(height));
        if self.total.patterns > self.limit.patterns {
            return Err(Spent);
        }
        let tile = Frame {
            transform: Transform::from_scale(sx, sy),
            max_bbox: frame.max_bbox,
            canvas: (width, height),
        };
        self.nodes(pattern.root(), tile)
    }
}

#[cfg(test)]
#[path = "pixels_tests.rs"]
mod tests;
