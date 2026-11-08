//! The symbol collection of own commands and bar buttons (RC-110-10): a subset of Fluent UI
//! System Icons that rCommander brings along, as Total Commander brings `WCMICONS.DLL`. An own
//! command's `icon_path` or a bar button's symbol names one as `rc-symbol:<name>` - its stable
//! upstream name, `folder_open`, not its code point - instead of a file; no migration, the field
//! is the same. The table (`symbols.tsv`: name, code point, category, English keywords) is
//! written with the font (`rcommander/resources/icons/rcommander-symbols.ttf`) by
//! `scripts/icon-font.sh --symbols`; the app draws a symbol as that font's glyph, and a name the
//! collection no longer has shows the button's text.
//!
//! Saving checks a reference against the collection (`usercmd.save`, `button_bar.save`): what
//! comes from outside - the window, an import, an agent - names a symbol that exists. Reading
//! keeps what is stored, so a collection that loses a name never makes a command unreadable.

use std::{collections::HashMap, sync::LazyLock};

/// What an `icon_path` starts with when it names a symbol of the collection.
pub const PREFIX: &str = "rc-symbol:";

/// The table, compiled in.
const TABLE: &str = include_str!("symbols.tsv");

/// A symbol's category: the picker's list on the left, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Files,
    Edit,
    Tools,
    Network,
    Development,
    Media,
    System,
    Arrows,
    Status,
}

impl Category {
    /// Every category, in the picker's order.
    pub const ALL: [Self; 9] = [
        Self::Files,
        Self::Edit,
        Self::Tools,
        Self::Network,
        Self::Development,
        Self::Media,
        Self::System,
        Self::Arrows,
        Self::Status,
    ];

    /// The table's word for it, and the end of its i18n key (`symbol_picker.category.<key>`).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Edit => "edit",
            Self::Tools => "tools",
            Self::Network => "network",
            Self::Development => "development",
            Self::Media => "media",
            Self::System => "system",
            Self::Arrows => "arrows",
            Self::Status => "status",
        }
    }

    fn of_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.key() == key)
    }
}

/// One symbol of the collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Symbol {
    /// The stable name, `folder_open`.
    pub name: &'static str,
    /// Its code point in the collection's font.
    pub glyph: char,
    pub category: Category,
    /// English words the search finds it by besides its name, blank-separated, lower case.
    pub keywords: &'static str,
}

impl Symbol {
    /// What an `icon_path` stores for it: `rc-symbol:<name>`.
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{PREFIX}{}", self.name)
    }

    /// Whether each of `words` (lower case) is part of its name or of a keyword.
    #[must_use]
    pub fn matches(&self, words: &[String]) -> bool {
        words
            .iter()
            .all(|word| self.name.contains(word.as_str()) || self.keywords.contains(word.as_str()))
    }
}

/// One row of the table; `None` for a comment, a blank line or a row that does not read.
fn row(line: &'static str) -> Option<Symbol> {
    if line.starts_with('#') {
        return None;
    }
    let mut fields = line.split('\t');
    let name = fields.next()?.trim();
    let point = fields.next()?.trim().strip_prefix("U+")?;
    let glyph = char::from_u32(u32::from_str_radix(point, 16).ok()?)?;
    let category = Category::of_key(fields.next()?.trim())?;
    let keywords = fields.next().unwrap_or_default().trim();
    (!name.is_empty()).then_some(Symbol {
        name,
        glyph,
        category,
        keywords,
    })
}

static SYMBOLS: LazyLock<Vec<Symbol>> = LazyLock::new(|| TABLE.lines().filter_map(row).collect());

static BY_NAME: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    SYMBOLS
        .iter()
        .enumerate()
        .map(|(at, symbol)| (symbol.name, at))
        .collect()
});

/// The collection, in the picker's order: by category, then as listed.
#[must_use]
pub fn all() -> &'static [Symbol] {
    &SYMBOLS
}

/// The symbol called `name`; `None` when the collection has none.
#[must_use]
pub fn find(name: &str) -> Option<&'static Symbol> {
    BY_NAME.get(name).and_then(|at| SYMBOLS.get(*at))
}

/// The place of the symbol called `name` in [`all`].
#[must_use]
pub fn position(name: &str) -> Option<usize> {
    BY_NAME.get(name).copied()
}

/// The name an `icon_path` refers to, when it is a reference (`rc-symbol:<name>`, blanks
/// around it ignored); `None` for a file or nothing.
#[must_use]
pub fn name_of(icon_path: &str) -> Option<&str> {
    icon_path.trim().strip_prefix(PREFIX).map(str::trim)
}

/// Whether `icon_path` names a symbol of the collection rather than a file - known or not.
#[must_use]
pub fn is_reference(icon_path: &str) -> bool {
    name_of(icon_path).is_some()
}

/// The symbol `icon_path` refers to; `None` for a file, nothing, or a name the collection does
/// not have (the button then shows its text).
#[must_use]
pub fn of_path(icon_path: &str) -> Option<&'static Symbol> {
    name_of(icon_path).and_then(find)
}

/// Checks an `icon_path` that is saved: a file or nothing passes, a reference must name a symbol
/// of the collection.
///
/// # Errors
///
/// What does not fit, for the error's reason: a reference to no symbol of the collection.
pub fn check(icon_path: &str) -> Result<(), String> {
    match name_of(icon_path) {
        Some(name) if find(name).is_none() => Err(format!(
            "the symbol names no symbol of the collection ({PREFIX}<name>)"
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "symbols_tests.rs"]
mod tests;
