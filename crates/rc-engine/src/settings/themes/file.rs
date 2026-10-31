//! A theme as a file (`*.rctheme`, RC-100-50): JSON with `format`, `version`, `name`, `dark`,
//! `accent` and `tokens`, written by the export and read by the import of the Themes tab.
//!
//! A file comes from anywhere, so it is checked whole before anything of it is used: at most
//! [`MAX_FILE_BYTES`], UTF-8 JSON, only the known keys, the format and the version this build
//! reads, `dark` a switch, the accent `system` or `#rrggbb`, every known token `#rrggbb`. The
//! name is cut to the name rule and loses its control characters; a token this build does not
//! know is dropped and named, so the window can say so. Nothing is written here: the window
//! saves what this module returns through `theme.save`.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use super::{Theme, ThemeAccent, clean_name, clean_tokens};

/// The value of `format`.
pub const FILE_FORMAT: &str = "rcommander-theme";
/// The version this build writes and reads.
pub const FILE_VERSION: u64 = 1;
/// The file ending, without the dot.
pub const FILE_EXTENSION: &str = "rctheme";
/// The largest file the import reads.
pub const MAX_FILE_BYTES: usize = 64 * 1024;

/// The keys a file has; any other refuses it.
const KEYS: [&str; 6] = ["format", "version", "name", "dark", "accent", "tokens"];

/// Why a file is no theme, each with a stable code (`error.<code>` in the window).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeFileError {
    #[error("the file is larger than 64 KiB")]
    TooLarge,
    #[error("the file is no theme: {0}")]
    Invalid(&'static str),
    #[error("the file has a key a theme does not have")]
    UnknownKey,
    #[error("the theme is of another version")]
    Version,
    #[error("a colour of the theme is no #rrggbb")]
    Colour,
}

impl ThemeFileError {
    /// Every code.
    pub const ALL_CODES: &[&str] = &[
        "theme_file_too_large",
        "theme_file_invalid",
        "theme_file_unknown_key",
        "theme_file_version",
        "theme_file_colour",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "theme_file_too_large",
            Self::Invalid(_) => "theme_file_invalid",
            Self::UnknownKey => "theme_file_unknown_key",
            Self::Version => "theme_file_version",
            Self::Colour => "theme_file_colour",
        }
    }
}

/// A theme read from a file: what to save, and the tokens dropped as unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTheme {
    pub name: String,
    pub dark: bool,
    pub accent: ThemeAccent,
    pub tokens: BTreeMap<String, String>,
    /// The tokens this build does not know, left out.
    pub dropped: Vec<String>,
}

/// The file as written.
#[derive(Serialize)]
struct Written<'a> {
    format: &'static str,
    version: u64,
    name: &'a str,
    dark: bool,
    accent: &'a str,
    tokens: &'a BTreeMap<String, String>,
}

/// `theme` as a file's bytes: its own tokens only, so a built-in's file is its name, mode and
/// accent - the window writes a built-in's colours in before (`theme/builtin/`).
#[must_use]
pub fn write_file(theme: &Theme) -> Vec<u8> {
    let written = Written {
        format: FILE_FORMAT,
        version: FILE_VERSION,
        name: &theme.name,
        dark: theme.dark,
        accent: theme.accent.as_text(),
        tokens: &theme.tokens,
    };
    let mut bytes = serde_json::to_vec_pretty(&written).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// The theme in `bytes`, checked whole.
///
/// # Errors
///
/// The file is too large, no JSON object, has a key a theme does not have, another format or
/// version, a blank name, or a colour that is no `#rrggbb`.
pub fn parse_file(bytes: &[u8]) -> Result<ImportedTheme, ThemeFileError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ThemeFileError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ThemeFileError::Invalid("not UTF-8"))?;
    let value: Value =
        serde_json::from_str(text).map_err(|_| ThemeFileError::Invalid("not JSON"))?;
    let object = value
        .as_object()
        .ok_or(ThemeFileError::Invalid("not an object"))?;
    if object.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err(ThemeFileError::UnknownKey);
    }
    if object.get("format").and_then(Value::as_str) != Some(FILE_FORMAT) {
        return Err(ThemeFileError::Invalid("not a theme's format"));
    }
    if object.get("version").and_then(Value::as_u64) != Some(FILE_VERSION) {
        return Err(ThemeFileError::Version);
    }
    let name = clean_name(
        object
            .get("name")
            .and_then(Value::as_str)
            .ok_or(ThemeFileError::Invalid("no name"))?,
    );
    if name.is_empty() {
        return Err(ThemeFileError::Invalid("a blank name"));
    }
    let dark = object
        .get("dark")
        .and_then(Value::as_bool)
        .ok_or(ThemeFileError::Invalid("no mode"))?;
    let accent = match object.get("accent") {
        None => ThemeAccent::System,
        Some(accent) => accent
            .as_str()
            .and_then(ThemeAccent::parse)
            .ok_or(ThemeFileError::Colour)?,
    };
    let empty = Map::new();
    let tokens = match object.get("tokens") {
        None => &empty,
        Some(tokens) => tokens
            .as_object()
            .ok_or(ThemeFileError::Invalid("the tokens are no object"))?,
    };
    let mut pairs = Vec::with_capacity(tokens.len());
    for (token, value) in tokens {
        pairs.push((
            token.as_str(),
            value.as_str().ok_or(ThemeFileError::Colour)?,
        ));
    }
    let (tokens, dropped) = clean_tokens(pairs).map_err(|_| ThemeFileError::Colour)?;
    Ok(ImportedTheme {
        name,
        dark,
        accent,
        tokens,
        dropped,
    })
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
