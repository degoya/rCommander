//! A viewer's output is hostile input, checked before the window sees it (`docs/plugins.md`,
//! "Validation of plugin output"): the outputs a document names, its title, a text's encoding, a
//! picture's format and size, token spans.
//!
//! New in rCommander; rDownloader's `component/convert.rs` at commit 26c8592a6 checks a
//! resolver's answer the same way (caps, refusal of what breaks a rule), not a viewer's.

use rc_plugin_api::viewer::{Document, Output, Token};
use rc_plugin_types::Failure;

use super::convert;
use crate::bindings::exports::rcommander::plugin::viewer as wit;
use crate::failure::{bad_output, readable};

/// Longest title shown.
pub const MAX_TITLE_CHARS: usize = 256;
/// Most pages a `pages` document may announce.
pub const MAX_PAGES: u32 = 1_000_000;
/// Most lines one `tokens` call asks for: a screen, generously.
pub const MAX_TOKEN_LINES: usize = 1024;
/// Most spans one line may carry.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// Longest encoding name `text` may answer (`utf-8`, `windows-1252`).
const MAX_ENCODING_CHARS: usize = 32;
/// The viewer's own picture limits (`rcommander::app::viewer::picture`, RC-020-07): a side and
/// a pixel count, so a raster that claims billions of pixels is refused before it is read.
pub const MAX_SIDE: u32 = 32 * 1024;
pub const MAX_PIXELS: u64 = 256 * 1024 * 1024;

/// What `image` wrote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Webp,
    /// Raw RGBA, exactly `width * height * 4` bytes.
    Rgba {
        width: u32,
        height: u32,
    },
}

/// The document `open` answered, held to the rules: outputs not empty, none twice, each listed
/// in the manifest; a page count for `pages`; the title readable and capped.
pub(crate) fn document(raw: wit::Document, offered: &[Output]) -> Result<Document, Failure> {
    if raw.outputs.is_empty() {
        return Err(bad_output("outputs_empty"));
    }
    let mut outputs: Vec<Output> = Vec::with_capacity(raw.outputs.len());
    for output in raw.outputs.into_iter().map(convert::output) {
        if outputs.contains(&output) {
            return Err(bad_output("outputs_duplicate"));
        }
        if !offered.contains(&output) {
            return Err(bad_output("output_not_in_manifest"));
        }
        outputs.push(output);
    }
    let pages = if outputs.contains(&Output::Pages) {
        match raw.pages {
            Some(count) if (1..=MAX_PAGES).contains(&count) => Some(count),
            _ => return Err(bad_output("page_count")),
        }
    } else {
        None
    };
    Ok(Document {
        outputs,
        title: raw.title.map(|title| readable(&title, MAX_TITLE_CHARS)),
        pages,
    })
}

/// The encoding `text` named: a label `encoding_rs` could know, lower-cased.
pub(crate) fn encoding(raw: &str) -> Result<String, Failure> {
    if raw.is_empty()
        || raw.len() > MAX_ENCODING_CHARS
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(bad_output("encoding"));
    }
    Ok(raw.to_ascii_lowercase())
}

/// The bytes of a `width` x `height` RGBA raster, if it is within the picture limits.
#[must_use]
pub fn raster_bytes(width: u32, height: u32) -> Option<u64> {
    let pixels = u64::from(width) * u64::from(height);
    (width > 0 && height > 0 && width <= MAX_SIDE && height <= MAX_SIDE && pixels <= MAX_PIXELS)
        .then_some(pixels.saturating_mul(4))
}

/// `png`, `jpeg`, `gif`, `bmp`, `webp` or `rgba:<w>x<h>`; anything else is no format.
#[must_use]
pub fn image_format(raw: &str) -> Option<ImageFormat> {
    Some(match raw {
        "png" => ImageFormat::Png,
        "jpeg" => ImageFormat::Jpeg,
        "gif" => ImageFormat::Gif,
        "bmp" => ImageFormat::Bmp,
        "webp" => ImageFormat::Webp,
        _ => {
            let (width, height) = raw.strip_prefix("rgba:")?.split_once('x')?;
            ImageFormat::Rgba {
                width: dimension(width)?,
                height: dimension(height)?,
            }
        }
    })
}

fn dimension(digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.len() > 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The picture `image` wrote: a known format, something written, a raster of exactly its size
/// and within the picture limits. An encoded picture's dimensions are the decoder's to check.
pub(crate) fn image(raw: &str, committed: u64) -> Result<ImageFormat, Failure> {
    let format = image_format(raw).ok_or_else(|| bad_output("image_format"))?;
    if let ImageFormat::Rgba { width, height } = format {
        let expected = raster_bytes(width, height).ok_or_else(|| bad_output("image_size"))?;
        if committed != expected {
            return Err(bad_output("raster_length"));
        }
    } else if committed == 0 {
        return Err(bad_output("image_empty"));
    }
    Ok(format)
}

/// The spans of the lines asked for, one entry per line: `None` where the plugin answered
/// nothing or broke a rule, so that line is shown uncoloured. The text is never changed by its
/// tokens; lists beyond the lines asked for are dropped.
pub(crate) fn tokens(lines: &[&str], answered: Vec<Vec<wit::Token>>) -> Vec<Option<Vec<Token>>> {
    let mut answered = answered.into_iter();
    lines
        .iter()
        .map(|line| answered.next().and_then(|spans| line_tokens(line, spans)))
        .collect()
}

/// One line's spans: sorted, not overlapping, not empty, inside the line and on character
/// boundaries, at most [`MAX_TOKENS_PER_LINE`].
fn line_tokens(line: &str, spans: Vec<wit::Token>) -> Option<Vec<Token>> {
    if spans.len() > MAX_TOKENS_PER_LINE {
        return None;
    }
    let mut checked = Vec::with_capacity(spans.len());
    let mut covered = 0_usize;
    for span in spans {
        let start = usize::try_from(span.start).ok()?;
        let end = start.checked_add(usize::try_from(span.len).ok()?)?;
        if span.len == 0
            || start < covered
            || end > line.len()
            || !line.is_char_boundary(start)
            || !line.is_char_boundary(end)
        {
            return None;
        }
        covered = end;
        checked.push(Token {
            start: span.start,
            len: span.len,
            class: convert::token_class(span.class),
        });
    }
    Some(checked)
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
