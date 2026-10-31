//! The dark look: WinUI's dark theme resources.

use iced::Color;

use super::{Tokens, hex, syntax, terminal};

/// The cards' and the panels' fill, and the text on them.
const SURFACE: u32 = 0x272727;
const TEXT: u32 = 0xFFFFFF;

/// The dark tokens, with `accent` (the system's accent for a dark background).
pub(super) fn tokens(accent: Color) -> Tokens {
    let surface = hex(SURFACE, 1.0);
    let text = hex(TEXT, 1.0);
    let text_secondary = hex(0xC5C5C5, 1.0);
    Tokens {
        dark: true,
        window: hex(0x202020, 1.0),
        surface,
        header: hex(0x2C2C2C, 1.0),
        stroke: hex(0x3A3A3A, 1.0),
        text,
        text_secondary,
        accent,
        cursor_fill: Color { a: 0.16, ..accent },
        cursor_fill_inactive: hex(0xFFFFFF, 0.07),
        selected_fill: Color { a: 0.07, ..accent },
        // About 4 % lighter than the surface, below the inactive cursor's 7 %.
        stripe_fill: hex(0xFFFFFF, 0.04),
        control: hex(0x2D2D2D, 1.0),
        // The lists' own colours start as the window's; the person's colours replace them
        // (RC-100-43, `colours.rs`).
        list_fill: surface,
        list_text: text,
        list_text_secondary: text_secondary,
        cursor_text: text,
        marked: accent,
        syntax: syntax::DARK,
        terminal: terminal::CAMPBELL,
    }
}
