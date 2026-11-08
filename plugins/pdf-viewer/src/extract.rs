//! A page's text through hayro's interpreter: a [`Device`] that draws nothing and hands every
//! glyph with a Unicode value to [`Layout`] at its position. Paths, images, clips and groups are
//! ignored; hayro still decodes what the content stream names, within the store's limits.
//!
//! The interface follows hayro-interpret 0.9's own `examples/extract_html.rs`.

use hayro_interpret::font::GlyphRun;
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::hayro_syntax::Pdf;
use hayro_interpret::kurbo::{Affine, BezPath, Point, Rect};
use hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};

use crate::layout::Layout;

/// The units per em of hayro's glyph space (`hayro_font::UNITS_PER_EM`, not re-exported).
const GLYPH_UNITS_PER_EM: f64 = 1000.0;

/// The device that collects a page's glyphs.
#[derive(Default)]
struct TextDevice {
    layout: Layout,
}

impl Device<'_> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'_>, _: &DrawMode) {}

    fn push_clip_path(&mut self, _: &ClipPath) {}

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}

    fn draw_glyph_run(&mut self, glyph_run: &GlyphRun<'_, '_>, props: DrawProps<'_>, _: &DrawMode) {
        for glyph in glyph_run.glyphs() {
            let Some(unicode) = glyph.as_unicode() else {
                continue;
            };
            let transform = props.transform * glyph.transform();
            let origin = transform * Point::ZERO;
            // A glyph's transform maps hayro's glyph space of 1000 units per em to user space.
            let [_, _, c, d, _, _] = transform.as_coeffs();
            let size = c.hypot(d) * GLYPH_UNITS_PER_EM;
            match unicode {
                BfString::Char(ch) => {
                    let mut buffer = [0u8; 4];
                    self.layout
                        .push(origin.x, origin.y, size, ch.encode_utf8(&mut buffer));
                }
                BfString::String(text) => self.layout.push(origin.x, origin.y, size, &text),
            }
        }
    }

    fn draw_image(&mut self, _: Image<'_, '_>, _: ImageDrawProps<'_>) {}

    fn pop_clip(&mut self) {}

    fn pop_transparency_group(&mut self) {}
}

/// The pages' text, one call of `page` per page in order until it answers `false`: the page's
/// index and its text, lines ending in a line break.
pub fn pages(pdf: &Pdf, mut page: impl FnMut(usize, String) -> bool) {
    let cache = InterpreterCache::new();
    for (index, shown) in pdf.pages().iter().enumerate() {
        // The bounding box and the initial transform do not matter for text: the glyphs'
        // positions are compared with each other only.
        let mut context = Context::new(
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, 1.0, 1.0),
            &cache,
            pdf.xref(),
            InterpreterSettings::default(),
        );
        let mut device = TextDevice::default();
        interpret_page(shown, &mut context, &mut device);
        if !page(index, device.layout.finish()) {
            break;
        }
    }
}
