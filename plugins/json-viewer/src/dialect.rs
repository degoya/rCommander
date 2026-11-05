//! The flavours of JSON the viewer reads (RC-100-80), chosen by the file's extension: plain JSON,
//! JSON with comments (`.jsonc`), JSON5 (`.json5`) and one value per line (`.jsonl`, `.ndjson`).
//!
//! - **JSONC** allows `//` and `/* */` comments and a comma before a closing bracket.
//! - **JSON5** allows the same and, beside them, keys without quotes, strings in single quotes,
//!   escapes of any character, `+`, hexadecimal numbers, a point at either end of a number,
//!   `Infinity` and `NaN`. A string continued over a line break (a backslash at the end of a
//!   line) is not read as one: it ends there, as `invalid`, like a JSON string.
//! - **JSON Lines** is JSON, a value per line, each line checked for itself: a broken line is an
//!   error of its own and the lines after it read as usual. The tree lists the lines' values;
//!   blank lines are left out.

/// How the bytes of a file are read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Json,
    Jsonc,
    Json5,
    /// `.jsonl` and `.ndjson`.
    Lines,
}

impl Dialect {
    /// The dialect of a file named `name` (never a path): by its extension, JSON for any other.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        match extension.as_deref() {
            Some("jsonc") => Self::Jsonc,
            Some("json5") => Self::Json5,
            Some("jsonl" | "ndjson") => Self::Lines,
            _ => Self::Json,
        }
    }

    /// `//` and `/* */` are comments.
    #[must_use]
    pub fn comments(self) -> bool {
        matches!(self, Self::Jsonc | Self::Json5)
    }

    /// A comma may stand before `}` or `]`.
    #[must_use]
    pub fn trailing_commas(self) -> bool {
        matches!(self, Self::Jsonc | Self::Json5)
    }

    /// JSON5's keys, strings and numbers.
    #[must_use]
    pub fn json5(self) -> bool {
        self == Self::Json5
    }

    /// A value per line, each line checked for itself.
    #[must_use]
    pub fn records(self) -> bool {
        self == Self::Lines
    }
}

#[cfg(test)]
#[path = "dialect_tests.rs"]
mod tests;
