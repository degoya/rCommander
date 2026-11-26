//! High contrast: black on white and white on black, a strong cursor - yellow with black text on
//! light, blue with white text on dark - and marks that stay readable on it; every pair far above
//! WCAG AA. While Windows' own high contrast is on, the window draws this theme in the system's
//! colours instead ([`with_system`], RC-110-79).

use iced::Color;

use super::Palette;
use crate::theme::{
    Syntax, Tokens,
    access::SystemColours,
    colours::{MIN_CONTRAST, contrast},
    hex,
    status::{self, readable},
};

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xFFFFFF,
    surface: 0xFFFFFF,
    header: 0xFFFFFF,
    stroke: 0x000000,
    text: 0x000000,
    text_secondary: 0x1A1A1A,
    accent: 0x00009F,
    cursor: 0xFFE000,
    cursor_inactive: 0xD0D0D0,
    selected: 0xF3E0FF,
    cursor_text: 0x000000,
    marked: 0x6A0080,
    list_fill: 0xFFFFFF,
    list_text: 0x000000,
    list_text_secondary: 0x1A1A1A,
    stripe: 0xF0F0F0,
    control: 0xFFFFFF,
    syntax: [
        0x0000C0, 0x00406A, 0xA00000, 0x006000, 0x800080, 0x3C5A00, 0x333333, 0x000000, 0x005F6A,
        0x5C3A00, 0xC00000,
    ],
    terminal: [
        0x000000, 0xFFFFFF, 0x000000, 0xA00000, 0x006000, 0x6A5000, 0x00009F, 0x800080, 0x005F6A,
        0xC0C0C0, 0x5C5C5C, 0xC00000, 0x007A00, 0x7A6400, 0x0000FF, 0xA000A0, 0x007A87, 0xFFFFFF,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x000000,
    surface: 0x000000,
    header: 0x000000,
    stroke: 0xFFFFFF,
    text: 0xFFFFFF,
    text_secondary: 0xE0E0E0,
    accent: 0xFFFF00,
    cursor: 0x0000C8,
    cursor_inactive: 0x505050,
    selected: 0x2A2A00,
    cursor_text: 0xFFFFFF,
    marked: 0xFFFF00,
    list_fill: 0x000000,
    list_text: 0xFFFFFF,
    list_text_secondary: 0xE0E0E0,
    stripe: 0x141414,
    control: 0x000000,
    syntax: [
        0x6FB8FF, 0x9CDCFE, 0xFFB080, 0xB5FFA8, 0xFF8CFF, 0x8CD878, 0xE0E0E0, 0xFFFFFF, 0x4EF0D0,
        0xFFFF80, 0xFF6A6A,
    ],
    terminal: [
        0xFFFFFF, 0x000000, 0x000000, 0xFF4040, 0x00FF00, 0xFFFF00, 0x4080FF, 0xFF40FF, 0x00FFFF,
        0xC0C0C0, 0x808080, 0xFF8080, 0x80FF80, 0xFFFF80, 0x80B0FF, 0xFF80FF, 0x80FFFF, 0xFFFFFF,
    ],
};

/// This theme in the colours of Windows' high-contrast theme `colours` (RC-110-79), of the mode
/// its window colour belongs to: every background in the window colour, the text and the lines
/// in its text colour, the active cursor in the highlight with its text, the inactive one in the
/// grey text, the accent and the marked names in the link colour. What the system leaves weak is
/// moved until it reads: the link colour and the syntax colours on the window, the inactive
/// cursor under the highlight's text. The terminal keeps this theme's ANSI colours on the
/// system's background.
pub(in crate::theme) fn with_system(colours: &SystemColours) -> Tokens {
    let c = |rgb: u32| hex(rgb, 1.0);
    let window = c(colours.window);
    let text = c(colours.window_text);
    let dark = contrast(window, Color::WHITE) > contrast(window, Color::BLACK);
    let mut tokens = if dark { DARK.tokens() } else { LIGHT.tokens() };
    let on_window = |colour: Color| readable(colour, dark, &[window], MIN_CONTRAST);
    let link = on_window(c(colours.hot_light));
    let cursor_text = c(colours.highlight_text);
    let light_cursor_text =
        contrast(cursor_text, Color::BLACK) > contrast(cursor_text, Color::WHITE);
    tokens.window = window;
    tokens.surface = window;
    tokens.header = window;
    tokens.list_fill = window;
    tokens.control = window;
    tokens.stripe_fill = window;
    tokens.selected_fill = window;
    tokens.stroke = text;
    tokens.text = text;
    tokens.text_secondary = text;
    tokens.list_text = text;
    tokens.list_text_secondary = text;
    tokens.accent = link;
    tokens.marked = link;
    tokens.cursor_fill = c(colours.highlight);
    tokens.cursor_text = cursor_text;
    tokens.cursor_fill_inactive = readable(
        c(colours.gray_text),
        !light_cursor_text,
        &[cursor_text],
        MIN_CONTRAST,
    );
    let syntax = tokens.syntax;
    tokens.syntax = Syntax {
        keyword: on_window(syntax.keyword),
        key: on_window(syntax.key),
        string: on_window(syntax.string),
        number: on_window(syntax.number),
        literal: on_window(syntax.literal),
        comment: on_window(syntax.comment),
        punctuation: on_window(syntax.punctuation),
        operator: on_window(syntax.operator),
        type_name: on_window(syntax.type_name),
        function: on_window(syntax.function),
        invalid: on_window(syntax.invalid),
    };
    tokens.terminal.foreground = text;
    tokens.terminal.background = window;
    status::fit(&mut tokens);
    tokens
}
