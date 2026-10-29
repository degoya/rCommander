use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{Color as AnsiColor, NamedColor, Rgb},
};
use iced::Color;

use super::{ColorPalette, Theme, hex_to_color};

#[test]
fn hex_colours_parse_and_bad_ones_do_not() {
    assert_eq!(hex_to_color("#ff8000"), Some(Color::from_rgb8(255, 128, 0)));
    for bad in ["", "GG", "#GG0000", "ff8000", "#ff80000", "#ff80"] {
        assert_eq!(hex_to_color(bad), None, "{bad:?}");
    }
}

#[test]
fn the_palette_names_the_first_sixteen_and_xterm_the_rest() {
    let palette = ColorPalette::default();
    let theme = Theme::new(&palette);
    let unset = Colors::default();
    let hex = |text: &str| hex_to_color(text).expect("a colour");
    assert_eq!(
        theme.color(AnsiColor::Indexed(1), &unset),
        hex(&palette.red)
    );
    assert_eq!(
        theme.color(AnsiColor::Named(NamedColor::BrightWhite), &unset),
        hex(&palette.bright_white)
    );
    assert_eq!(
        theme.color(AnsiColor::Named(NamedColor::DimCyan), &unset),
        hex(&palette.dim_cyan)
    );
    assert_eq!(
        theme.color(AnsiColor::Named(NamedColor::Foreground), &unset),
        hex(&palette.foreground)
    );
    assert_eq!(
        theme.color(AnsiColor::Indexed(196), &unset),
        Color::from_rgb8(255, 0, 0)
    );
    assert_eq!(
        theme.color(AnsiColor::Indexed(255), &unset),
        Color::from_rgb8(238, 238, 238)
    );
}

#[test]
fn a_colour_the_program_set_wins() {
    let theme = Theme::new(&ColorPalette::default());
    let mut set = Colors::default();
    let rgb = Rgb { r: 1, g: 2, b: 3 };
    set[NamedColor::Background] = Some(rgb);
    set[4] = Some(rgb);
    assert_eq!(
        theme.color(AnsiColor::Named(NamedColor::Background), &set),
        Color::from_rgb8(1, 2, 3)
    );
    assert_eq!(
        theme.color(AnsiColor::Indexed(4), &set),
        Color::from_rgb8(1, 2, 3)
    );
}

#[test]
fn a_broken_palette_entry_falls_back_instead_of_panicking() {
    let palette = ColorPalette {
        red: "red".into(),
        ..ColorPalette::default()
    };
    let theme = Theme::new(&palette);
    let foreground = hex_to_color(&palette.foreground).expect("a colour");
    assert_eq!(
        theme.color(AnsiColor::Indexed(1), &Colors::default()),
        foreground
    );
}
