//! Which names a typed text matches: Total Commander's quick search (type-ahead, Ctrl+Alt with
//! letters) and quick filter (Ctrl+S).
//!
//! Both read `*` and `?` as wildcards. A quick search matches names that start with the text, or
//! contain it anywhere when the setting says so; case is ignored unless the setting says
//! otherwise. [`find`] is where the cursor goes: the entry under it while it still matches, else
//! the first match from the top; ArrowDown and ArrowUp step to the next and the previous match.
//! A filter keeps the names that contain its text, or - with a wildcard in it - the names the
//! whole pattern matches (`*.txt`); case is ignored. With `quick_search.filters` the quick search
//! filters: its text becomes a filter of the names it would find ([`quick_pattern`]). The window
//! types, the engine decides.

use rc_vfs::Entry;

use crate::mask::{lowercase, wildcard};

/// The longest text a quick search or a filter takes; a longer one is refused.
pub const MAX_PATTERN: usize = 256;

/// Where a quick search looks for its match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seek {
    /// The entry under the cursor while it matches, else the first match from the top.
    Here,
    /// The next match below the cursor.
    Next,
    /// The previous match above the cursor.
    Previous,
}

/// The quick search's settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchOptions {
    /// Anywhere in the name instead of at its start.
    pub anywhere: bool,
    /// Upper and lower case differ.
    pub case_sensitive: bool,
}

/// A typed text as a matcher of names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameMatch {
    /// The wildcard pattern; in lower case unless `case_sensitive`.
    pattern: Vec<char>,
    case_sensitive: bool,
}

impl NameMatch {
    /// The quick search for `text`: names starting with it, or containing it with
    /// [`MatchOptions::anywhere`]. `None` for an empty text.
    #[must_use]
    pub fn quick_search(text: &str, options: MatchOptions) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        Some(Self::new(
            &quick_pattern(text, options.anywhere),
            options.case_sensitive,
        ))
    }

    /// The quick filter `text`: names containing it, or with `*` or `?` in it the names the whole
    /// pattern matches. Case is ignored. `None` for a text of blanks only.
    #[must_use]
    pub fn filter(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let pattern = if text.contains(['*', '?']) {
            text.to_owned()
        } else {
            format!("*{text}*")
        };
        Some(Self::new(&pattern, false))
    }

    fn new(pattern: &str, case_sensitive: bool) -> Self {
        let pattern = if case_sensitive {
            pattern.chars().collect()
        } else {
            lowercase(pattern).collect()
        };
        Self {
            pattern,
            case_sensitive,
        }
    }

    /// Whether `name` matches; nothing is allocated for it.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        if self.case_sensitive {
            wildcard(&self.pattern, name.chars())
        } else {
            wildcard(&self.pattern, lowercase(name))
        }
    }
}

/// The wildcard pattern of the quick search `text`: the names starting with it, or containing it
/// `anywhere`. As a filter ([`NameMatch::filter`]) it keeps the names the quick search finds, the
/// filter ignoring case; empty for an empty text.
#[must_use]
pub fn quick_pattern(text: &str, anywhere: bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut pattern = String::with_capacity(text.len() + 2);
    if anywhere && !text.starts_with('*') {
        pattern.push('*');
    }
    pattern.push_str(text);
    if !text.ends_with('*') {
        pattern.push('*');
    }
    pattern
}

/// The index of the entry a quick search moves the cursor to from `cursor`; `None` when no
/// entry in that direction matches, and the cursor stays.
#[must_use]
pub fn find(entries: &[Entry], matcher: &NameMatch, cursor: usize, seek: Seek) -> Option<usize> {
    let hit = |index: &usize| matcher.matches(&entries[*index].name);
    let len = entries.len();
    match seek {
        Seek::Here => Some(cursor)
            .filter(|at| *at < len && hit(at))
            .or_else(|| (0..len).find(hit)),
        Seek::Next => (cursor.saturating_add(1)..len).find(hit),
        Seek::Previous => (0..cursor.min(len)).rev().find(hit),
    }
}

/// The entries `matcher` keeps, in their order.
pub(crate) fn filtered(entries: &[Entry], matcher: &NameMatch) -> Vec<Entry> {
    entries
        .iter()
        .filter(|entry| matcher.matches(&entry.name))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "quick_search_tests.rs"]
mod tests;
