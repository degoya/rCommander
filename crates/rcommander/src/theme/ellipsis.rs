//! Shortening a path or a file name in the middle so it fits its column (`docs/design.md`): the
//! start and the end stay, an ellipsis takes the place of what is cut. A file list's names are
//! cut at their end instead while `appearance.name_cut` says so ([`end_ellipsis_by`]).
//!
//! The width is measured, not counted: [`middle_ellipsis_by`] takes the measure, and
//! [`middle_ellipsis`] uses [`estimate_width`], the advances of the interface font (Segoe UI) per
//! class of character at a font size. A view does not hold the renderer, so the estimate stands
//! in for shaping; it errs on the wide side, and the container clips what is left over. A list in
//! a monospaced font (RC-010-17) measures with [`estimate_mono_width`] (`theme::fonts`).
//!
//! The cut falls between characters, never inside one, and keeps a character with the combining
//! marks, variation selectors and emoji modifiers after it and a zero-width-joiner sequence whole:
//! close to grapheme clusters, without a segmentation dependency.

use std::borrow::Cow;

/// What stands for the cut part.
pub const ELLIPSIS: &str = "\u{2026}";

/// `text` in at most `max_width` pixels of the interface font at `size`: unchanged when it fits,
/// else its start, [`ELLIPSIS`] and its end.
pub fn middle_ellipsis(text: &str, max_width: f32, size: f32) -> Cow<'_, str> {
    middle_ellipsis_by(text, max_width, |part| estimate_width(part, size))
}

/// [`middle_ellipsis`] with the width of a piece of text from `width`; the width of a run is the
/// sum of its characters' widths. The end gets half the room first, the start what is left, then
/// the end again what the start left over.
pub fn middle_ellipsis_by(text: &str, max_width: f32, width: impl Fn(&str) -> f32) -> Cow<'_, str> {
    let clusters = clusters(text);
    let widths: Vec<f32> = clusters
        .iter()
        .map(|&(start, end)| width(&text[start..end]))
        .collect();
    if widths.iter().sum::<f32>() <= max_width {
        return Cow::Borrowed(text);
    }
    let budget = max_width - width(ELLIPSIS);
    if budget.is_nan() || budget <= 0.0 {
        return Cow::Borrowed(ELLIPSIS);
    }
    // Clusters `..head` and `tail..` stay.
    let (mut head, mut tail, mut used) = (0, clusters.len(), 0.0);
    while tail > head && used + widths[tail - 1] <= budget / 2.0 {
        tail -= 1;
        used += widths[tail];
    }
    while head < tail && used + widths[head] <= budget {
        used += widths[head];
        head += 1;
    }
    while tail > head && used + widths[tail - 1] <= budget {
        tail -= 1;
        used += widths[tail];
    }
    let head_end = if head == 0 { 0 } else { clusters[head - 1].1 };
    let tail_start = clusters.get(tail).map_or(text.len(), |&(start, _)| start);
    Cow::Owned(format!(
        "{}{ELLIPSIS}{}",
        &text[..head_end],
        &text[tail_start..]
    ))
}

/// `text` in at most `max_width` by `width`: unchanged when it fits, else as much of its start as
/// fits beside [`ELLIPSIS`], then the ellipsis.
pub fn end_ellipsis_by(text: &str, max_width: f32, width: impl Fn(&str) -> f32) -> Cow<'_, str> {
    let clusters = clusters(text);
    let widths: Vec<f32> = clusters
        .iter()
        .map(|&(start, end)| width(&text[start..end]))
        .collect();
    if widths.iter().sum::<f32>() <= max_width {
        return Cow::Borrowed(text);
    }
    let budget = max_width - width(ELLIPSIS);
    let (mut head, mut used) = (0, 0.0);
    while head < clusters.len() && used + widths[head] <= budget {
        used += widths[head];
        head += 1;
    }
    let head_end = if head == 0 { 0 } else { clusters[head - 1].1 };
    Cow::Owned(format!("{}{ELLIPSIS}", &text[..head_end]))
}

/// The width of `text` in pixels in the interface font at `size`, estimated from Segoe UI's
/// advances per class of character (in em, rounded up): narrow punctuation and `i`/`l`, the
/// half-narrow `f`/`t`/`r` and brackets, ordinary lower case and digits, upper case, the wide
/// `m`/`w`/`M`/`W`, and East Asian and emoji characters a full em. Joining marks take no room.
pub fn estimate_width(text: &str, size: f32) -> f32 {
    text.chars().map(advance).sum::<f32>() * size
}

/// The width of `text` in pixels in a monospaced font at `size` (RC-010-17): one advance per
/// character, two for an East Asian or emoji one, none for a joining mark. The advance errs on
/// the wide side of Cascadia Mono's and Consolas'.
pub fn estimate_mono_width(text: &str, size: f32) -> f32 {
    text.chars().map(mono_advance).sum::<f32>() * size
}

/// A monospaced font's advance in em: the widest of the usual ones, rounded up.
const MONO_ADVANCE: f32 = 0.62;

fn mono_advance(c: char) -> f32 {
    match c {
        _ if joins_previous(c) || c.is_control() => 0.0,
        _ if is_wide(c) => 2.0 * MONO_ADVANCE,
        _ => MONO_ADVANCE,
    }
}

/// A character's estimated advance in em.
fn advance(c: char) -> f32 {
    match c {
        _ if joins_previous(c) || c.is_control() => 0.0,
        ' ' | 'i' | 'j' | 'l' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' | '`' => 0.30,
        'f' | 't' | 'r' | 'I' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '\\' | '-' | '"' => 0.40,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.90,
        '&' | '#' => 0.68,
        _ if c.is_ascii_uppercase() => 0.68,
        _ if c.is_ascii() => 0.58,
        _ if is_wide(c) => 1.0,
        _ => 0.70,
    }
}

/// East Asian wide and full-width characters and the emoji blocks: a full em.
fn is_wide(c: char) -> bool {
    matches!(
        u32::from(c),
        0x1100..=0x115F
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1F64F
            | 0x1F680..=0x1F6FF
            | 0x1F900..=0x1F9FF
            | 0x20000..=0x3FFFD
    )
}

/// A character drawn with the one before it: combining marks, the zero-width joiner, variation
/// selectors, emoji skin-tone modifiers and tag characters.
fn joins_previous(c: char) -> bool {
    matches!(
        u32::from(c),
        0x0300..=0x036F
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x200D
            | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0x1F3FB..=0x1F3FF
            | 0xE0020..=0xE007F
            | 0xE0100..=0xE01EF
    )
}

/// The byte ranges of `text`'s clusters: a character with the joining characters after it, and
/// the character after a zero-width joiner.
fn clusters(text: &str) -> Vec<(usize, usize)> {
    let mut clusters: Vec<(usize, usize)> = Vec::new();
    let mut after_joiner = false;
    for (start, c) in text.char_indices() {
        let end = start + c.len_utf8();
        if (after_joiner || joins_previous(c))
            && let Some(last) = clusters.last_mut()
        {
            last.1 = end;
        } else {
            clusters.push((start, end));
        }
        after_joiner = c == '\u{200D}';
    }
    clusters
}

#[cfg(test)]
#[path = "ellipsis_tests.rs"]
mod tests;
