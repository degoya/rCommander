//! The settings of `manifest.toml` (`[[settings]]`), read once in `open`: a change applies from
//! the next file opened.

use resvg::tiny_skia::Color;

/// The setting `size`: the picture's longest side in pixels. 2048 is the most: 2048 by 2048 RGBA
/// is 16 MiB, well inside the store's memory beside the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    /// The setting's value; anything but `512` or `2048` is the default, `1024`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("512") => Self::Small,
            Some("2048") => Self::Large,
            _ => Self::Medium,
        }
    }

    /// The longest side in pixels.
    #[must_use]
    pub fn longest(self) -> u32 {
        match self {
            Self::Small => 512,
            Self::Medium => 1024,
            Self::Large => 2048,
        }
    }
}

/// The setting `background`: what shows where the picture draws nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Background {
    #[default]
    Transparent,
    White,
    Black,
}

impl Background {
    /// The setting's value; anything but `white` or `black` is the default, `transparent`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("white") => Self::White,
            Some("black") => Self::Black,
            _ => Self::Transparent,
        }
    }

    /// The colour the canvas is filled with first; none for `transparent`.
    #[must_use]
    pub fn color(self) -> Option<Color> {
        match self {
            Self::Transparent => None,
            Self::White => Some(Color::WHITE),
            Self::Black => Some(Color::BLACK),
        }
    }
}

/// Both settings of one document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub size: Size,
    pub background: Background,
}

impl Settings {
    /// The settings from their stored values, the default for one missing or unknown.
    #[must_use]
    pub fn from_values(size: Option<&str>, background: Option<&str>) -> Self {
        Self {
            size: Size::from_setting(size),
            background: Background::from_setting(background),
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
