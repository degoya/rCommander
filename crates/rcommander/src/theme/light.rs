//! The light look: WinUI's light theme resources.

use iced::Color;

use super::{Tokens, hex, syntax};

/// The cards' and the panels' fill, and the text on them.
const SURFACE: u32 = 0xFFFFFF;
const TEXT: u32 = 0x1A1A1A;

/// The light tokens, with `accent` (the system's accent for a light background).
pub(super) fn tokens(accent: Color) -> Tokens {
    let surface = hex(SURFACE, 1.0);
    let text = hex(TEXT, 1.0);
    let text_secondary = hex(0x616161, 1.0);
    Tokens {
        dark: false,
        window: hex(0xF3F3F3, 1.0),
        surface,
        header: hex(0xF9F9F9, 1.0),
        stroke: hex(0xE5E5E5, 1.0),
        text,
        text_secondary,
        accent,
        cursor_fill: Color { a: 0.12, ..accent },
        cursor_fill_inactive: hex(0x000000, 0.06),
        selected_fill: Color { a: 0.06, ..accent },
        // About 3 % darker than the surface, below the inactive cursor's 6 %.
        stripe_fill: hex(0x000000, 0.03),
        control: hex(0xFBFBFB, 1.0),
        // The lists' own colours start as the window's; the person's colours replace them
        // (RC-100-43, `colours.rs`).
        list_fill: surface,
        list_text: text,
        list_text_secondary: text_secondary,
        cursor_text: text,
        marked: accent,
        syntax: syntax::LIGHT,
    }
}
