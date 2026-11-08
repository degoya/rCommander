//! The setting of `manifest.toml` (`[[settings]]`), read once in `open`: a change applies from
//! the next file opened.

/// The setting `sheet_rows`: how many rows below its header row a sheet shows in `text`; the
/// rest is counted in one closing line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetRows {
    Thousand,
    #[default]
    TenThousand,
    HundredThousand,
}

impl SheetRows {
    /// The setting's value; anything but `1000` or `100000` is the default, `10000`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("1000") => Self::Thousand,
            Some("100000") => Self::HundredThousand,
            _ => Self::TenThousand,
        }
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(self) -> u64 {
        match self {
            Self::Thousand => 1_000,
            Self::TenThousand => 10_000,
            Self::HundredThousand => 100_000,
        }
    }
}

/// The plugin's settings, read once when a document opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub sheet_rows: SheetRows,
}

impl Settings {
    /// The settings from their stored values, the default for one missing or unknown.
    #[must_use]
    pub fn from_values(sheet_rows: Option<&str>) -> Self {
        Self {
            sheet_rows: SheetRows::from_setting(sheet_rows),
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
