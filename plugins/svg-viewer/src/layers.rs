//! Where resvg 0.49 puts its intermediate canvases, computed as resvg computes them: the canvas
//! of an isolated group (`render_group`, `render_layer`, `resvg-0.49.0/src/render.rs:49-153`),
//! the largest one a drawing allows (`max_filter_bbox`, `src/lib.rs:86-97`), a filter's region
//! and the canvas of an `feImage` (`apply_inner`, `apply_image`, `src/filter/mod.rs:357-384,
//! 873-901`). `None` wherever resvg draws nothing.

use resvg::tiny_skia::{IntRect, Transform};
use resvg::usvg::Group;
use resvg::usvg::filter::{Filter, Kind, Primitive};

/// The largest canvas resvg allows a layer on a canvas of `size`: four times the canvas around
/// it, clamped (`max_filter_bbox`).
#[must_use]
pub fn max_bbox(size: (u32, u32)) -> Option<IntRect> {
    IntRect::from_xywh(
        i32::try_from(size.0).unwrap_or(i32::MAX).saturating_mul(-2),
        i32::try_from(size.1).unwrap_or(i32::MAX).saturating_mul(-2),
        size.0.saturating_mul(5),
        size.1.saturating_mul(5),
    )
    .or_else(|| IntRect::from_ltrb(i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2))
}

/// `rect` within `bounds` (`fit_to_rect`, `src/geom.rs:5-30`).
#[must_use]
pub fn fit(rect: IntRect, bounds: IntRect) -> Option<IntRect> {
    IntRect::from_ltrb(
        rect.left().max(bounds.left()),
        rect.top().max(bounds.top()),
        rect.right().min(bounds.right()),
        rect.bottom().min(bounds.bottom()),
    )
}

/// The canvas of an isolated group.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    /// Its place and size.
    pub rect: IntRect,
    /// The transform its content, filters, clip path and mask are drawn at.
    pub transform: Transform,
    /// The largest canvas its content may take in turn.
    pub max_bbox: IntRect,
}

/// The canvas resvg draws the isolated `group` into, at `transform` (the group's own included)
/// on a drawing whose largest layer is `max_bbox`.
#[must_use]
pub fn layer(group: &Group, transform: Transform, max_bbox: IntRect) -> Option<Layer> {
    let filtered = !group.filters().is_empty();
    // A filter is drawn without the rotation and skew, which are applied when the layer is.
    let rest = if filtered {
        let sx = transform.sx.hypot(transform.ky);
        let sy = transform.kx.hypot(transform.sy);
        Transform::from_row(
            transform.sx / sx,
            transform.ky / sx,
            transform.kx / sy,
            transform.sy / sy,
            0.0,
            0.0,
        )
    } else {
        Transform::identity()
    };
    let inverse = rest.invert()?;
    let max_bbox = max_bbox.to_rect().transform(inverse)?.round_out()?;
    let transform = inverse.pre_concat(transform);
    let bbox = group.layer_bounding_box().transform(transform)?;
    let rect = if filtered {
        // The filter region is the canvas: no margin.
        IntRect::from_xywh(
            bbox.x().floor() as i32,
            bbox.y().floor() as i32,
            bbox.width().ceil().max(1.0) as u32,
            bbox.height().ceil().max(1.0) as u32,
        )?
    } else {
        // Two pixels of margin on each side for the anti-aliased edge.
        IntRect::from_xywh(
            (bbox.x().floor() as i32).checked_sub(2)?,
            (bbox.y().floor() as i32).checked_sub(2)?,
            (bbox.width().ceil() as u32).checked_add(4)?,
            (bbox.height().ceil() as u32).checked_add(4)?,
        )?
    };
    let rect = fit(rect, max_bbox)?;
    // The content is shifted to the canvas' corner, as resvg does (`shift_ts`).
    let dx = bbox.x() - (bbox.x() - rect.x() as f32);
    let dy = bbox.y() - (bbox.y() - rect.y() as f32);
    Some(Layer {
        rect,
        transform: Transform::from_translate(-dx, -dy).pre_concat(transform),
        max_bbox,
    })
}

/// The region a filter draws in on a source canvas of `source` at `transform`, within the
/// source, and the transform its primitives are drawn at.
#[must_use]
pub fn filter_region(
    filter: &Filter,
    transform: Transform,
    source: (u32, u32),
) -> Option<(IntRect, Transform)> {
    let region = filter.rect().transform(transform)?.to_int_rect();
    let region = fit(region, IntRect::from_xywh(0, 0, source.0, source.1)?)?;
    let transform = transform.post_translate(-region.x() as f32, -region.y() as f32);
    Some((region, transform))
}

/// The canvases a primitive takes besides its result: a copy of each input it reads, which it
/// takes from the source or from a result it shares (`get_input`, `Image::take`). At most two,
/// and a merge one per node.
#[must_use]
pub fn inputs(kind: &Kind) -> u64 {
    match kind {
        Kind::Flood(_) | Kind::Image(_) | Kind::Turbulence(_) => 0,
        Kind::Blend(_) | Kind::Composite(_) | Kind::DisplacementMap(_) => 2,
        Kind::Merge(merge) => merge.inputs().len() as u64,
        _ => 1,
    }
}

/// The transform and largest layer an `feImage` primitive draws its content at, into a canvas
/// of the filter's `region` (`apply_image`).
#[must_use]
pub fn image(
    primitive: &Primitive,
    region: IntRect,
    transform: Transform,
) -> Option<(Transform, IntRect)> {
    let subregion = primitive.rect().transform(transform)?.to_int_rect();
    let (sx, sy) = transform.get_scale();
    let transform =
        Transform::from_row(sx, 0.0, 0.0, sy, subregion.x() as f32, subregion.y() as f32);
    Some((
        transform,
        IntRect::from_xywh(0, 0, region.width(), region.height())?,
    ))
}
