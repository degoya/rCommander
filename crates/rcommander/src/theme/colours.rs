//! Colours read and judged (RC-100-43, RC-100-50): `#rrggbb` as a colour, a translucent fill as
//! it is seen, and WCAG 2's contrast with the pairs of a look the Themes tab warns about. The
//! colours of a look are its theme's (`themes.rs`); the eight list colours of RC-100-43 became the
//! Custom themes (`rc_engine::settings::themes::adopt_colours`).

use iced::Color;
use rc_engine::settings::colour_value;

use super::Tokens;

/// `#rrggbb` (or `#rgb`) as a colour; `None` for anything else and for an empty value.
#[must_use]
pub fn parse_hex(text: &str) -> Option<Color> {
    let value = colour_value(text).filter(|value| !value.is_empty())?;
    let rgb = u32::from_str_radix(value.trim_start_matches('#'), 16).ok()?;
    Some(Color::from_rgb8(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    ))
}

/// `top` drawn over the opaque `base`: a translucent fill as it is seen.
#[must_use]
pub fn over(top: Color, base: Color) -> Color {
    let mix = |high: f32, low: f32| high * top.a + low * (1.0 - top.a);
    Color::from_rgb(mix(top.r, base.r), mix(top.g, base.g), mix(top.b, base.b))
}

/// The contrast ratio of two opaque colours, WCAG 2's definition: 1 to 21.
#[must_use]
pub fn contrast(first: Color, second: Color) -> f32 {
    let (first, second) = (luminance(first), luminance(second));
    let (light, dark) = if first > second {
        (first, second)
    } else {
        (second, first)
    };
    (light + 0.05) / (dark + 0.05)
}

/// The smallest contrast WCAG AA asks of normal text.
pub const MIN_CONTRAST: f32 = 4.5;

fn luminance(colour: Color) -> f32 {
    let channel = |value: f32| {
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b)
}

/// The pairs of a look below [`MIN_CONTRAST`], as i18n keys of what they are: text on the
/// background, on the second background, on the cursor, a marked name on the background, the
/// window's text on its surface, the terminal's foreground on its background.
#[must_use]
pub fn low_contrast(tokens: &Tokens) -> Vec<&'static str> {
    let fill = tokens.list_fill;
    let terminal = tokens.terminal;
    let pairs = [
        ("settings.colours.low_surface", tokens.text, tokens.surface),
        (
            "settings.colours.low_terminal",
            terminal.foreground,
            terminal.background,
        ),
        ("settings.colours.low_text", tokens.list_text, fill),
        (
            "settings.colours.low_stripe",
            tokens.list_text,
            over(tokens.stripe_fill, fill),
        ),
        (
            "settings.colours.low_cursor",
            tokens.cursor_text,
            over(tokens.cursor_fill, fill),
        ),
        ("settings.colours.low_marked", tokens.marked, fill),
    ];
    pairs
        .into_iter()
        .filter(|(_, text, back)| contrast(*text, *back) < MIN_CONTRAST)
        .map(|(what, ..)| what)
        .collect()
}

#[cfg(test)]
#[path = "colours_tests.rs"]
mod tests;
