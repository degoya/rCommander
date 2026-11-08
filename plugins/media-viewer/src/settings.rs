//! The settings of `manifest.toml` (`[[settings]]`), read once in `open`: a change applies from
//! the next file opened.

/// The setting `cover`: whether an embedded cover is offered as the `image` output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cover {
    #[default]
    On,
    Off,
}

impl Cover {
    /// The setting's value; anything but `off` is the default, `on`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("off") => Self::Off,
            _ => Self::On,
        }
    }
}

/// The plugin's settings of one document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub cover: Cover,
}

impl Settings {
    /// The settings from their stored values, the default for one missing or unknown.
    #[must_use]
    pub fn from_values(cover: Option<&str>) -> Self {
        Self {
            cover: Cover::from_setting(cover),
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
