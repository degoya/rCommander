//! The terminal's colours: a palette of sixteen colours plus foreground and background as the
//! app gives them, xterm's 256-colour cube and grey ramp, and the colours a program set itself
//! (OSC 4, 10, 11) over both. Taken from iced_term 0.8 (`theme.rs`, MIT); the palette is parsed
//! once instead of on every cell, and a colour that does not parse falls back instead of
//! panicking.

use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{Color as AnsiColor, NamedColor, Rgb},
};
use iced::{Color, widget::container};

/// The colours as `#rrggbb`, as iced_term takes them.
#[derive(Debug, Clone)]
pub struct ColorPalette {
    pub foreground: String,
    pub background: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
    pub bright_foreground: Option<String>,
    pub dim_foreground: String,
    pub dim_black: String,
    pub dim_red: String,
    pub dim_green: String,
    pub dim_yellow: String,
    pub dim_blue: String,
    pub dim_magenta: String,
    pub dim_cyan: String,
    pub dim_white: String,
}

impl Default for ColorPalette {
    fn default() -> Self {
        let s = str::to_owned;
        Self {
            foreground: s("#d8d8d8"),
            background: s("#181818"),
            black: s("#181818"),
            red: s("#ac4242"),
            green: s("#90a959"),
            yellow: s("#f4bf75"),
            blue: s("#6a9fb5"),
            magenta: s("#aa759f"),
            cyan: s("#75b5aa"),
            white: s("#d8d8d8"),
            bright_black: s("#6b6b6b"),
            bright_red: s("#c55555"),
            bright_green: s("#aac474"),
            bright_yellow: s("#feca88"),
            bright_blue: s("#82b8c8"),
            bright_magenta: s("#c28cb8"),
            bright_cyan: s("#93d3c3"),
            bright_white: s("#f8f8f8"),
            bright_foreground: None,
            dim_foreground: s("#828482"),
            dim_black: s("#0f0f0f"),
            dim_red: s("#712b2b"),
            dim_green: s("#5f6f3a"),
            dim_yellow: s("#a17e4d"),
            dim_blue: s("#456877"),
            dim_magenta: s("#704d68"),
            dim_cyan: s("#4d7770"),
            dim_white: s("#8e8e8e"),
        }
    }
}

/// The palette, parsed.
#[derive(Debug, Clone)]
pub(crate) struct Theme {
    foreground: Color,
    background: Color,
    bright_foreground: Color,
    dim_foreground: Color,
    /// The 256 indexed colours: the palette's sixteen, the cube, the grey ramp.
    indexed: [Color; 256],
    /// Black to white, dimmed.
    dim: [Color; 8],
}

impl Theme {
    pub(crate) fn new(palette: &ColorPalette) -> Self {
        let foreground = parse(&palette.foreground, Color::WHITE);
        let background = parse(&palette.background, Color::BLACK);
        let mut indexed = [Color::BLACK; 256];
        let sixteen = [
            &palette.black,
            &palette.red,
            &palette.green,
            &palette.yellow,
            &palette.blue,
            &palette.magenta,
            &palette.cyan,
            &palette.white,
            &palette.bright_black,
            &palette.bright_red,
            &palette.bright_green,
            &palette.bright_yellow,
            &palette.bright_blue,
            &palette.bright_magenta,
            &palette.bright_cyan,
            &palette.bright_white,
        ];
        for (slot, hex) in indexed.iter_mut().zip(sixteen) {
            *slot = parse(hex, foreground);
        }
        let level = |step: u8| if step == 0 { 0 } else { step * 40 + 55 };
        for r in 0..6u8 {
            for g in 0..6u8 {
                for b in 0..6u8 {
                    let index = 16 + usize::from(r) * 36 + usize::from(g) * 6 + usize::from(b);
                    indexed[index] = Color::from_rgb8(level(r), level(g), level(b));
                }
            }
        }
        for step in 0..24u8 {
            let value = step * 10 + 8;
            indexed[232 + usize::from(step)] = Color::from_rgb8(value, value, value);
        }
        let dim = [
            &palette.dim_black,
            &palette.dim_red,
            &palette.dim_green,
            &palette.dim_yellow,
            &palette.dim_blue,
            &palette.dim_magenta,
            &palette.dim_cyan,
            &palette.dim_white,
        ]
        .map(|hex| parse(hex, foreground));
        Self {
            foreground,
            background,
            bright_foreground: palette
                .bright_foreground
                .as_deref()
                .map_or(foreground, |hex| parse(hex, foreground)),
            dim_foreground: parse(&palette.dim_foreground, foreground),
            indexed,
            dim,
        }
    }

    /// The colour of `color`, where the program did not set it in `set`.
    pub(crate) fn color(&self, color: AnsiColor, set: &Colors) -> Color {
        match color {
            AnsiColor::Spec(rgb) => from_rgb(rgb),
            AnsiColor::Indexed(index) => {
                set[usize::from(index)].map_or(self.indexed[usize::from(index)], from_rgb)
            }
            AnsiColor::Named(named) => set[named].map_or_else(|| self.named(named), from_rgb),
        }
    }

    pub(crate) fn background(&self) -> Color {
        self.background
    }

    fn named(&self, named: NamedColor) -> Color {
        let index = named as usize;
        match named {
            NamedColor::Foreground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::BrightForeground => self.bright_foreground,
            NamedColor::DimForeground => self.dim_foreground,
            NamedColor::DimBlack
            | NamedColor::DimRed
            | NamedColor::DimGreen
            | NamedColor::DimYellow
            | NamedColor::DimBlue
            | NamedColor::DimMagenta
            | NamedColor::DimCyan
            | NamedColor::DimWhite => {
                let first = NamedColor::DimBlack as usize;
                self.dim
                    .get(index - first)
                    .copied()
                    .unwrap_or(self.foreground)
            }
            // Black to BrightWhite are 0 to 15; the cursor takes the foreground.
            _ => self.indexed.get(index).copied().unwrap_or(self.foreground),
        }
    }

    /// The widget's own background, painted under the cells.
    pub(crate) fn container_style(&self) -> container::Style {
        container::Style {
            background: Some(self.background.into()),
            ..container::Style::default()
        }
    }
}

fn from_rgb(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

/// `#rrggbb` as a colour, or `fallback`.
fn parse(hex: &str, fallback: Color) -> Color {
    hex_to_color(hex).unwrap_or(fallback)
}

pub(crate) fn hex_to_color(hex: &str) -> Option<Color> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some(Color::from_rgb8(channel(0)?, channel(2)?, channel(4)?))
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
