//! The picture: usvg reads the document with nothing but the file itself, resvg draws it onto a
//! canvas of the setting's size, and the canvas leaves as straight RGBA.

use std::sync::Arc;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, FontResolver, ImageHrefResolver, Options, fontdb};

use crate::budget::{self, Cost, Remaining, read_as_svg};
use crate::code;
use crate::depth::{MAX_DEPTH, depth_within, tree_within};
use crate::doctype::{data_without_doctype, without_doctype};
use crate::load::is_gzip;
use crate::pixels::{Pixels, canvas_pixels};
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
/// parsed. An SVG inside a `data:` URL draws no image of its own: usvg 0.49 would hand it this
/// resolver again, one nesting inside the next, each with its own recursion on the stack. Each
/// time usvg reads such an SVG, its expanded elements are taken from `remaining`; one that does
/// not fit is not read.
#[must_use]
pub fn options(remaining: Arc<Remaining>) -> Options<'static> {
    let mut fonts = fontdb::Database::new();
    fonts.load_font_data(FONT.to_vec());
    let data_only = ImageHrefResolver::default_data_resolver();
    Options {
        resources_dir: None,
        fontdb: Arc::new(fonts),
        font_resolver: bundled_font(),
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(move |mime, data, options| {
                let raster = RASTER_MAGIC.iter().any(|magic| data.starts_with(magic));
                if is_gzip(&data) || (!raster && !depth_within(&data, MAX_DEPTH)) {
                    return None;
                }
                // usvg reads an SVG here with DTDs: it gets the bytes without one (RC-080-69).
                let data = if read_as_svg(mime) {
                    let data = data_without_doctype(data);
                    if !remaining.take_svg(&data) {
                        return None;
                    }
                    data
                } else {
                    data
                };
                data_only(mime, data, &without_images(Arc::clone(&options.fontdb)))
            }),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    }
}

/// The bundled font for every `font-family`.
fn bundled_font() -> FontResolver<'static> {
    FontResolver {
        select_font: Box::new(|_, fonts| fonts.faces().next().map(|face| face.id)),
        ..FontResolver::default()
    }
}

/// The options of an SVG inside a `data:` URL: the same font, and no image resolved at all, as
/// usvg 0.45 read one.
fn without_images(fontdb: Arc<fontdb::Database>) -> Options<'static> {
    Options {
        resources_dir: None,
        fontdb,
        font_resolver: bundled_font(),
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    }
}

/// The picture of `svg` (UTF-8, a leading byte-order mark allowed) at `settings`: `not_svg` for
/// a document usvg cannot read, one using an entity of its DTD or one without a size,
/// `too_complex` nested beyond the depth budget (told before parsing and on the tree), beyond
/// the parse's node budget, beyond the element or arc curve budget (`<use>`, markers, patterns
/// and embedded SVGs; [`Cost::LIMIT`]), or with pattern tiles or intermediate canvases beyond
/// the pixel budgets ([`Pixels::LIMIT`]).
pub fn render(svg: &[u8], settings: Settings) -> Result<Pixmap, Failure> {
    let text = std::str::from_utf8(svg).map_err(|_| not_svg())?;
    let text = without_doctype(text.strip_prefix(BOM).unwrap_or(text));
    if !depth_within(text.as_bytes(), MAX_DEPTH) {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    // Without a DTD and within the node budget while it is parsed, the depth counted once more
    // on the tree (RC-080-69).
    let document =
        roxmltree::Document::parse_with_options(&text, budget::parsing()).map_err(|error| {
            match error {
                roxmltree::Error::NodesLimitReached => {
                    Failure::coded(FailureKind::BadData, code::TOO_COMPLEX)
                }
                _ => not_svg(),
            }
        })?;
    if !tree_within(&document, MAX_DEPTH) {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    let count = budget::expanded(&document, Cost::LIMIT);
    if count.exceeds(Cost::LIMIT) {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    let remaining = Arc::new(Remaining::new(Cost {
        elements: Cost::LIMIT.elements - count.elements,
        curves: Cost::LIMIT.curves - count.curves,
    }));
    let tree = usvg::Tree::from_xmltree(&document, &options(Arc::clone(&remaining)))
        .map_err(|_| not_svg())?;
    drop(document);
    if remaining.exceeded() {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    let size = tree.size();
    let (width, height) =
        fit(size.width(), size.height(), settings.size.longest()).ok_or_else(not_svg)?;
    let scale = Transform::from_scale(width as f32 / size.width(), height as f32 / size.height());
    if canvas_pixels(&tree, (width, height), scale, Pixels::LIMIT).exceeds(Pixels::LIMIT) {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_COMPLEX));
    }
    let mut pixmap = Pixmap::new(width, height).ok_or_else(not_svg)?;
    if let Some(color) = settings.background.color() {
        pixmap.fill(color);
    }
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
