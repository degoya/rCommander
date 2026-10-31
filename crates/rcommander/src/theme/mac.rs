//! The macOS look: the system's greys of light and dark mode - window,
//! list and header as Finder and the system settings draw them, the separators, the label
//! colours - instead of WinUI's. The accent is the one chosen in System Settings
//! (`system.rs`). Used while `appearance.macos_look` is on ([`super::mac_look`], RC-100-49).

use iced::Color;

use super::{Tokens, hex, syntax, terminal};

/// The Mac's fonts behind [`super::UI_FONT`] and [`super::MONO_FONT`], the generic sans-serif
/// and monospace families: San Francisco (/System/Library/Fonts/SFNS.ttf) and Menlo. Set before
/// the first text is shaped; without them the families are cosmic-text's defaults, as on Linux.
pub(super) fn fonts() {
    let mut system = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = system.raw().db_mut();
    db.set_sans_serif_family("System Font");
    db.set_monospace_family("Menlo");
}

/// The light tokens with `accent` (`controlAccentColor` for a light background).
pub(super) fn light(accent: Color) -> Tokens {
    let surface = hex(0xFFFFFF, 1.0);
    let text = hex(0x1D1D1F, 1.0);
    let text_secondary = hex(0x6E6E73, 1.0);
    Tokens {
        dark: false,
        window: hex(0xECECEC, 1.0),
        surface,
        header: hex(0xF5F5F5, 1.0),
        stroke: hex(0xDCDCDC, 1.0),
        text,
        text_secondary,
        accent,
        cursor_fill: Color { a: 0.22, ..accent },
        cursor_fill_inactive: hex(0x000000, 0.08),
        selected_fill: Color { a: 0.10, ..accent },
        // Finder's alternating rows.
        stripe_fill: hex(0x000000, 0.035),
        control: hex(0xFFFFFF, 1.0),
        list_fill: surface,
        list_text: text,
        list_text_secondary: text_secondary,
        cursor_text: text,
        marked: accent,
        syntax: syntax::LIGHT,
        terminal: terminal::ONE_HALF_LIGHT,
    }
}

/// The dark tokens with `accent` (`controlAccentColor` for a dark background).
pub(super) fn dark(accent: Color) -> Tokens {
    let surface = hex(0x1E1E1E, 1.0);
    let text = hex(0xE5E5E7, 1.0);
    let text_secondary = hex(0x98989D, 1.0);
    Tokens {
        dark: true,
        window: hex(0x2A2A2A, 1.0),
        surface,
        header: hex(0x2E2E2E, 1.0),
        stroke: hex(0x3D3D3D, 1.0),
        text,
        text_secondary,
        accent,
        cursor_fill: Color { a: 0.38, ..accent },
        cursor_fill_inactive: hex(0xFFFFFF, 0.10),
        selected_fill: Color { a: 0.16, ..accent },
        stripe_fill: hex(0xFFFFFF, 0.03),
        control: hex(0x3A3A3C, 1.0),
        list_fill: surface,
        list_text: text,
        list_text_secondary: text_secondary,
        cursor_text: hex(0xFFFFFF, 1.0),
        marked: accent,
        syntax: syntax::DARK,
        terminal: terminal::CAMPBELL,
    }
}
