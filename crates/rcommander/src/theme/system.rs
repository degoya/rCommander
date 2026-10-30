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
#[cfg_attr(
    target_os = "macos",
    allow(dead_code, reason = "macOS reads its own accent")
)]
pub const DEFAULT_ACCENT: Accent = Accent {
    light: hex(0x005FB8, 1.0),
    dark: hex(0x60CDFF, 1.0),
};

/// The palette's shade for a light theme (Dark1) and for a dark one (Light2).
#[cfg_attr(
    target_os = "macos",
    allow(dead_code, reason = "macOS reads its own accent")
)]
const LIGHT_SHADE: usize = 4;
#[cfg_attr(
    target_os = "macos",
    allow(dead_code, reason = "macOS reads its own accent")
)]
const DARK_SHADE: usize = 1;

/// This user's accent; read once at the start.
pub fn accent() -> Accent {
    #[cfg(target_os = "macos")]
    {
        mac_accent()
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(windows)]
        let palette = windows::accent_palette();
        #[cfg(not(windows))]
        let palette: Option<Vec<u8>> = None;
        palette
            .as_deref()
            .and_then(from_palette)
            .unwrap_or(DEFAULT_ACCENT)
    }
}

/// The accent of an `AccentPalette` value: 32 bytes, eight RGBA shades.
#[cfg_attr(
    target_os = "macos",
    allow(dead_code, reason = "macOS reads its own accent")
)]
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

/// macOS: the accent chosen in System Settings, Appearance (`AppleAccentColor` in the global
/// domain; missing = multicolour, which draws blue), as `controlAccentColor` is for a light and a
/// dark background.
#[cfg(target_os = "macos")]
fn mac_accent() -> Accent {
    let chosen = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleAccentColor"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.trim().parse::<i32>().ok());
    let (light, dark) = match chosen {
        Some(-1) => (0x8C8C8C, 0x989898), // graphite
        Some(0) => (0xE0383E, 0xFF5257),  // red
        Some(1) => (0xF7821B, 0xF7821B),  // orange
        Some(2) => (0xFCB827, 0xFFC600),  // yellow
        Some(3) => (0x62BA46, 0x62BA46),  // green
        Some(5) => (0x953D96, 0xA550A7),  // purple
        Some(6) => (0xF74F9E, 0xF74F9E),  // pink
        _ => (0x007AFF, 0x0A84FF),        // blue
    };
    Accent {
        light: hex(light, 1.0),
        dark: hex(dark, 1.0),
    }
}
