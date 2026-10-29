//! The picture: usvg reads the document with nothing but the file itself, resvg draws it onto a
//! canvas of the setting's size, and the canvas leaves as straight RGBA.

use std::sync::Arc;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, FontResolver, ImageHrefResolver, Options, fontdb};

use crate::budget::{self, MAX_EXPANDED_ELEMENTS};
use crate::code;
use crate::depth::{MAX_DEPTH, depth_within};
use crate::load::is_gzip;
use crate::settings::Settings;
use crate::types::{Failure, FailureKind, Sink};

/// Lato Regular (OFL-1.1, `fonts/OFL.txt`): the only font, so text looks the same everywhere and
/// no system font is looked for. Its copyright and licence are in its own `name` table too.
pub static FONT: &[u8] = include_bytes!("../fonts/Lato-Regular.ttf");

/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;

const BOM: &str = "\u{feff}";

/// The signatures of the raster formats usvg draws from a `data:` URL; anything else it may read
/// as SVG.
const RASTER_MAGIC: [&[u8]; 4] = [b"\x89PNG", b"\xff\xd8", b"GIF8", b"RIFF"];

/// What usvg may use: the bundled font for every `font-family`, `data:` images only - an `href`
/// to a file or a URL resolves to nothing, a gzip stream inside a `data:` URL is not inflated
/// (usvg would do it without a limit), and an SVG inside one nested beyond [`MAX_DEPTH`] is not
/// parsed.
#[must_use]
pub fn options() -> Options<'static> {
    let mut fonts = fontdb::Database::new();
    fonts.load_font_data(FONT.to_vec());
    let data_only = ImageHrefResolver::default_data_resolver();
    Options {
        resources_dir: None,
        fontdb: Arc::new(fonts),
        font_resolver: FontResolver {
            select_font: Box::new(|_, fonts| fonts.faces().next().map(|face| face.id)),
            ..FontResolver::default()
        },
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(move |mime, data, options| {
                let raster = RASTER_MAGIC.iter().any(|magic| data.starts_with(magic));
                if is_gzip(&data) || (!raster && !depth_within(&data, MAX_DEPTH)) {
                    None
                } else {
                    data_only(mime, data, options)
                }
            }),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    }
}

/// The picture of `svg` (UTF-8, a leading byte-order mark allowed) at `settings`: `not_svg` for
/// a document usvg cannot read or one without a size, `too_complex` nested beyond the depth
/// budget (told before parsing) or beyond the `<use>` budget.
pub fn render(svg: &[u8], settings: Settings) -> Result<Pixmap, Failure> {
    let text = std::str::from_utf8(svg).map_err(|_| not_svg())?;
    let text = text.strip_prefix(BOM).unwrap_or(text);
    if !depth_within(text.as_bytes(), MAX_DEPTH) {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    let parsing = roxmltree::ParsingOptions {
        // As usvg itself reads: editors declare entities in a DTD; roxmltree expands them with
        // its own limits and never fetches one.
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let document = roxmltree::Document::parse_with_options(text, parsing).map_err(|_| not_svg())?;
    if budget::expanded_elements(&document, MAX_EXPANDED_ELEMENTS) > MAX_EXPANDED_ELEMENTS {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    let tree = usvg::Tree::from_xmltree(&document, &options()).map_err(|_| not_svg())?;
    drop(document);
    let size = tree.size();
    let (width, height) =
        fit(size.width(), size.height(), settings.size.longest()).ok_or_else(not_svg)?;
    let mut pixmap = Pixmap::new(width, height).ok_or_else(not_svg)?;
    if let Some(color) = settings.background.color() {
        pixmap.fill(color);
    }
    let scale = Transform::from_scale(width as f32 / size.width(), height as f32 / size.height());
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    Ok(pixmap)
}

/// The canvas for a picture of `width` by `height` user units whose longest side is `longest`
/// pixels, the aspect ratio kept and no side below one pixel; `None` for a size that is not one.
#[must_use]
pub fn fit(width: f32, height: f32, longest: u32) -> Option<(u32, u32)> {
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return None;
    }
    let longest = longest as f32;
    let scale = longest / width.max(height);
    let side = |length: f32| (length * scale).round().clamp(1.0, longest) as u32;
    Some((side(width), side(height)))
}

/// The canvas into `out` as straight (not premultiplied) RGBA, row by row, in writes of at most
/// [`WRITE_CHUNK`].
pub fn write_rgba(pixmap: &Pixmap, out: &mut impl Sink) -> Result<(), Failure> {
    let mut offset = 0u64;
    let mut chunk = Vec::with_capacity(WRITE_CHUNK);
    for pixels in pixmap.pixels().chunks(WRITE_CHUNK / 4) {
        chunk.clear();
        for pixel in pixels {
            let color = pixel.demultiply();
            chunk.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
        }
        out.write_at(offset, &chunk)?;
        offset += chunk.len() as u64;
    }
    Ok(())
}

fn not_svg() -> Failure {
    Failure::coded(FailureKind::BadData, code::NOT_SVG)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
