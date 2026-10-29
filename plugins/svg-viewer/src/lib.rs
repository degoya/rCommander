//! rCommander's bundled SVG viewer (RC-100-28): an SVG or SVGZ drawn as a picture (`image`) or
//! its source coloured as XML (`highlighted`), the picture first.
//!
//! Built on what the host promises a viewer (`docs/plugins.md`, "Viewer (WLX)"):
//!
//! - **The picture is a raw raster.** [`Viewer::image`] renders with resvg into straight RGBA and
//!   names it `rgba:<w>x<h>`; there is no PNG encoder in the plugin. The longest side is the
//!   setting `size`, the aspect ratio stays, and an SVG without `width` and `height` takes its
//!   `viewBox`. The viewer's own picture limits apply as to any picture.
//! - **Nothing outside the file.** Only `data:` images are drawn; an `href` to a file or to
//!   `http(s)` is not followed ([`render`]). Text is set in the one font the plugin brings (Lato
//!   Regular, `fonts/`), whatever `font-family` asks for: the sandbox has no system fonts.
//! - **Bounded work.** A file and an inflated `.svgz` are at most [`MAX_SVG_BYTES`]; a document
//!   nested deeper than [`depth::MAX_DEPTH`] is refused before it is parsed, one whose `<use>`
//!   references would expand beyond [`budget::MAX_EXPANDED_ELEMENTS`] elements before usvg
//!   builds it. What is left ends at the host's time, fuel and memory limits,
//!   as a notice.
//! - **The source is the XML viewer's.** [`Viewer::text`] and [`Viewer::tokens`] are those of
//!   `rc-plugin-xml-viewer` (RC-100-15) over the file's bytes, inflated for `.svgz`: the same
//!   token classes, the text UTF-8 through `rc-plugin-text` with each invalid part as U+FFFD and
//!   token offsets on that text.
//!
//! The layout is the other bundled viewers': the logic here, tested natively with `cargo test`;
//! the WIT translation in `src/guest.rs`, only on `wasm32`. The viewer world's types are the XML
//! viewer's ([`types`]), so a document of one is a document of the other.

pub mod budget;
pub mod depth;
#[cfg(target_arch = "wasm32")]
mod guest;
pub mod load;
pub mod render;
pub mod settings;

#[cfg(test)]
mod test_support;

use std::rc::Rc;

pub use rc_plugin_xml_viewer::types;

use load::Memory;
use rc_plugin_xml_viewer::Viewer as XmlViewer;
use settings::Settings;
use types::{Document, Failure, Format, Output, Sink, Source, Token};

/// The most bytes of SVG the plugin reads: the file, and an `.svgz` inflated.
pub const MAX_SVG_BYTES: u64 = 64 << 20;

/// The failure codes of `locales/en.json`, under the plugin's id.
pub mod code {
    /// The file or the inflated `.svgz` is larger than [`super::MAX_SVG_BYTES`].
    pub const TOO_LARGE: &str = "too_large";
    /// An `.svgz` that does not inflate.
    pub const BAD_GZIP: &str = "bad_gzip";
    /// Not an SVG usvg can read, or a picture without a size.
    pub const NOT_SVG: &str = "not_svg";
    /// Nesting deeper than the depth budget, or `<use>` references that would expand beyond the
    /// element budget.
    pub const TOO_COMPLEX: &str = "too_complex";
}

/// How far `probe` looks for the `<svg` start tag: past a long comment or DOCTYPE an editor
/// writes before it.
const PROBE_BYTES: u64 = 64 << 10;

/// Whether the file is an SVG: gzip (an `.svgz`, checked when it is opened), or `<svg` in its
/// first 64 KiB. rCommander matches the manifest's extensions itself; this is `probe`, so an XML
/// file named `.svg` goes to the next viewer.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(0);
    let Ok(head) = src.read_at(0, length) else {
        return false;
    };
    load::is_gzip(&head) || head.windows(4).any(|window| window == b"<svg")
}

/// One open document: the SVG's bytes (inflated) and the XML viewer's index over them.
pub struct Viewer {
    svg: Rc<Vec<u8>>,
    settings: Settings,
    source: XmlViewer<Memory>,
}

impl Viewer {
    /// `open`: reads the file, inflates an `.svgz` and indexes the source; offers `image`, then
    /// `highlighted`. The picture is drawn when `image` is asked for.
    pub fn open<S: Source>(mut src: S, settings: Settings) -> Result<(Self, Document), Failure> {
        let raw = load::read_all(&mut src, MAX_SVG_BYTES)?;
        let svg = Rc::new(if load::is_gzip(&raw) {
            load::inflate(&raw, MAX_SVG_BYTES)?
        } else {
            raw
        });
        let (source, _) = XmlViewer::open(Memory::new(Rc::clone(&svg)), Format::AsIs)?;
        let viewer = Self {
            svg,
            settings,
            source,
        };
        Ok((viewer, Self::document()))
    }

    /// The outputs every document offers, `image` first.
    #[must_use]
    pub fn document() -> Document {
        Document {
            outputs: vec![Output::Image, Output::Highlighted],
            title: None,
            pages: None,
        }
    }

    /// The SVG as read, an `.svgz` inflated.
    #[must_use]
    pub fn svg(&self) -> &[u8] {
        &self.svg
    }

    /// `image`: the picture as straight RGBA into `out`, named `rgba:<w>x<h>`.
    pub fn image(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let pixmap = render::render(&self.svg, self.settings)?;
        render::write_rgba(&pixmap, out)?;
        Ok(format!("rgba:{}x{}", pixmap.width(), pixmap.height()))
    }

    /// `text`: the source as UTF-8 into `out`.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        self.source.text(out)
    }

    /// `tokens`: the tokens of lines `first_line..first_line + count` of the source.
    pub fn tokens(&mut self, first_line: u32, count: u32) -> Result<Vec<Vec<Token>>, Failure> {
        self.source.tokens(first_line, count)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
