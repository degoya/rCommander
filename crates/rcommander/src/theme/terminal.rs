//! The terminal's colours (RC-100-50): foreground, background and the eight normal and eight
//! bright ANSI colours of a look, as a theme sets them; the dimmed colours follow from them. Fluent
//! keeps Windows Terminal's Campbell on dark and One Half Light on light, as before themes.

use iced::Color;

use super::{Tokens, hex};

/// A look's terminal colours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Terminal {
    pub foreground: Color,
    pub background: Color,
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,
    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_magenta: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,
}

impl Terminal {
    /// The colours of `rgb`: foreground, background, the eight normal and the eight bright ones.
    pub(super) const fn of(rgb: [u32; 18]) -> Self {
        Self {
            foreground: hex(rgb[0], 1.0),
            background: hex(rgb[1], 1.0),
            black: hex(rgb[2], 1.0),
            red: hex(rgb[3], 1.0),
            green: hex(rgb[4], 1.0),
            yellow: hex(rgb[5], 1.0),
            blue: hex(rgb[6], 1.0),
            magenta: hex(rgb[7], 1.0),
            cyan: hex(rgb[8], 1.0),
            white: hex(rgb[9], 1.0),
            bright_black: hex(rgb[10], 1.0),
            bright_red: hex(rgb[11], 1.0),
            bright_green: hex(rgb[12], 1.0),
            bright_yellow: hex(rgb[13], 1.0),
            bright_blue: hex(rgb[14], 1.0),
            bright_magenta: hex(rgb[15], 1.0),
            bright_cyan: hex(rgb[16], 1.0),
            bright_white: hex(rgb[17], 1.0),
        }
    }
}

/// Windows Terminal's Campbell, the dark looks' terminal (its background a step lighter).
pub(super) const CAMPBELL: Terminal = Terminal::of([
    0xCCCCCC, 0x1C1C1C, 0x0C0C0C, 0xC50F1F, 0x13A10E, 0xC19C00, 0x0037DA, 0x881798, 0x3A96DD,
    0xCCCCCC, 0x767676, 0xE74856, 0x16C60C, 0xF9F1A5, 0x3B78FF, 0xB4009E, 0x61D6D6, 0xF2F2F2,
]);

/// One Half Light, the light looks' terminal.
pub(super) const ONE_HALF_LIGHT: Terminal = Terminal::of([
    0x383A42, 0xFFFFFF, 0x383A42, 0xE45649, 0x50A14F, 0xC18401, 0x0184BC, 0xA626A4, 0x0997B3,
    0xFAFAFA, 0x4F525D, 0xDF6C75, 0x98C379, 0xE4C07A, 0x61AFEF, 0xC577DD, 0x56B5C1, 0xFFFFFF,
]);

/// `colour` as `#rrggbb`, as the terminal widget takes it.
fn css(colour: Color) -> String {
    let [red, green, blue, _] = colour.into_rgba8();
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// The terminal's colours of `tokens`' look; the dimmed ones are the normal ones, the dimmed
/// foreground the bright black, as Windows Terminal draws them.
pub fn terminal_palette(tokens: &Tokens) -> rc_term_widget::ColorPalette {
    let t = &tokens.terminal;
    rc_term_widget::ColorPalette {
        foreground: css(t.foreground),
        background: css(t.background),
        black: css(t.black),
        red: css(t.red),
        green: css(t.green),
        yellow: css(t.yellow),
        blue: css(t.blue),
        magenta: css(t.magenta),
        cyan: css(t.cyan),
        white: css(t.white),
        bright_black: css(t.bright_black),
        bright_red: css(t.bright_red),
        bright_green: css(t.bright_green),
        bright_yellow: css(t.bright_yellow),
        bright_blue: css(t.bright_blue),
        bright_magenta: css(t.bright_magenta),
        bright_cyan: css(t.bright_cyan),
        bright_white: css(t.bright_white),
        bright_foreground: None,
        dim_foreground: css(t.bright_black),
        dim_black: css(t.black),
        dim_red: css(t.red),
        dim_green: css(t.green),
        dim_yellow: css(t.yellow),
        dim_blue: css(t.blue),
        dim_magenta: css(t.magenta),
        dim_cyan: css(t.cyan),
        dim_white: css(t.white),
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
