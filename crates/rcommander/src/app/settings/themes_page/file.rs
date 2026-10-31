//! The `.rctheme` files of the Themes tab (RC-100-50): the path the field names, the file read and
//! checked - at most `MAX_FILE_BYTES`, never more read - and a theme written, both off the window's
//! thread. A built-in theme is written with its colours as they are seen, so the file draws the
//! same on another installation.

use std::{collections::BTreeMap, path::PathBuf};

use rc_engine::settings::themes::{
    FILE_EXTENSION, ImportedTheme, MAX_FILE_BYTES, Theme, ThemeAccent, parse_file, write_file,
};
use tokio::io::AsyncReadExt as _;

use crate::theme::{
    self,
    themes::{flatten, hex_of},
};

/// The file the field names: an absolute path without control characters; for an export with
/// the ending `.rctheme` added where it has another. `None` for anything else.
pub fn path_of(text: &str, export: bool) -> Option<PathBuf> {
    let text = text.trim();
    let path = PathBuf::from(text);
    if text.is_empty() || text.chars().any(char::is_control) || !path.is_absolute() {
        return None;
    }
    let ends = path
        .extension()
        .is_some_and(|ending| ending.eq_ignore_ascii_case(FILE_EXTENSION));
    if !export || ends {
        return Some(path);
    }
    let mut named = path.into_os_string();
    named.push(".");
    named.push(FILE_EXTENSION);
    Some(PathBuf::from(named))
}

/// The theme in the file at `path`, checked whole; the code of why not otherwise.
pub async fn read(path: PathBuf) -> Result<ImportedTheme, &'static str> {
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| "theme_file_unreadable")?;
    let cap = u64::try_from(MAX_FILE_BYTES).unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    file.take(cap + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "theme_file_unreadable")?;
    parse_file(&bytes).map_err(|error| error.code())
}

/// `bytes` written to `path`; the path written, or the code of why not.
pub async fn write(path: PathBuf, bytes: Vec<u8>) -> Result<String, &'static str> {
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|_| "theme_file_unwritable")?;
    Ok(path.to_string_lossy().into_owned())
}

/// The accent and the tokens of `theme` as an own theme keeps them: an own theme's as they are, a
/// built-in one's colours as they are seen - all but those made of the system's accent, where it
/// follows it. What Duplicate and Export write.
pub fn own_colours(theme: &Theme) -> (ThemeAccent, BTreeMap<String, String>) {
    if !theme.builtin {
        return (theme.accent.clone(), theme.tokens.clone());
    }
    let look = theme::themes::look(theme);
    let follows = theme::builtin::follows_system_accent(&theme.id);
    let accent = if follows {
        ThemeAccent::System
    } else {
        ThemeAccent::Colour(hex_of(look.accent))
    };
    (accent, flatten(look, follows))
}

/// `theme` as a file holds it; a built-in one under its name in the interface's language.
pub fn exported(theme: &Theme) -> Vec<u8> {
    let (accent, tokens) = own_colours(theme);
    let name = theme::builtin::name(&theme.id)
        .filter(|_| theme.builtin)
        .map_or_else(|| theme.name.clone(), str::to_owned);
    write_file(&Theme {
        name,
        accent,
        tokens,
        builtin: false,
        ..theme.clone()
    })
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
