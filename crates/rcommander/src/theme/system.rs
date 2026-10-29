//! The system's accent colour, as WinUI uses it: Windows keeps a palette of eight shades of the
//! user's accent (`AccentPalette`, `HKCU\...\Explorer\Accent`, eight RGBA quadruples from
//! lightest to darkest); a light theme draws with the first darker shade, a dark theme with the
//! second lighter one. Elsewhere, or when the value is missing or malformed, Windows' default
//! blue. The light or dark mode itself comes from iced ([`iced::system::theme`]), which follows
//! Windows' `AppsUseLightTheme` and its changes while the app runs.

#[cfg(windows)]
mod windows;

use iced::Color;

use super::hex;

/// The accent for each look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accent {
    pub light: Color,
    pub dark: Color,
}

/// Windows' default blue: `SystemAccentColorDark1` and `SystemAccentColorLight2` of #0078D4.
pub const DEFAULT_ACCENT: Accent = Accent {
    light: hex(0x005FB8, 1.0),
    dark: hex(0x60CDFF, 1.0),
};

/// The palette's shade for a light theme (Dark1) and for a dark one (Light2).
const LIGHT_SHADE: usize = 4;
const DARK_SHADE: usize = 1;

/// This user's accent; read once at the start.
pub fn accent() -> Accent {
    #[cfg(windows)]
    let palette = windows::accent_palette();
    #[cfg(not(windows))]
    let palette: Option<Vec<u8>> = None;
    palette
        .as_deref()
        .and_then(from_palette)
        .unwrap_or(DEFAULT_ACCENT)
}

/// The accent of an `AccentPalette` value: 32 bytes, eight RGBA shades.
pub fn from_palette(bytes: &[u8]) -> Option<Accent> {
    if bytes.len() != 32 {
        return None;
    }
    let shade = |index: usize| {
        let at = index * 4;
        Color::from_rgb8(bytes[at], bytes[at + 1], bytes[at + 2])
    };
    Some(Accent {
        light: shade(LIGHT_SHADE),
        dark: shade(DARK_SHADE),
    })
}
