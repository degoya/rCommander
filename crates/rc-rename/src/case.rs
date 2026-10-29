//! The case modes of the name and the extension.

use serde::{Deserialize, Serialize};

/// How the case of a rendered name or extension changes; Total Commander's five modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseMode {
    /// As the mask rendered it.
    #[default]
    Unchanged,
    /// `all lower case`.
    Lower,
    /// `ALL UPPER CASE`.
    Upper,
    /// `First character upper case, the rest lower case`.
    FirstUpper,
    /// `First Character Of Every Word Upper Case`; a word starts after any character that is
    /// not a letter or a digit.
    WordsUpper,
}

impl CaseMode {
    /// `text` in this case.
    #[must_use]
    pub fn apply(self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        self.append(&mut out, text);
        out
    }

    /// Appends `text` to `out` in this case, as if the two were one text: a word (for
    /// `FirstUpper`, the text) starts only where `out` does not already continue one. This is
    /// how `[U]`, `[L]`, `[F]` and `[n]` in a mask apply to what follows them.
    pub(crate) fn append(self, out: &mut String, text: &str) {
        match self {
            Self::Unchanged => out.push_str(text),
            Self::Lower => out.push_str(&text.to_lowercase()),
            Self::Upper => out.push_str(&text.to_uppercase()),
            Self::FirstUpper | Self::WordsUpper => {
                let mut first = out.is_empty();
                let mut word_start = out.chars().next_back().is_none_or(|c| !c.is_alphanumeric());
                for c in text.chars() {
                    let upper = if self == Self::FirstUpper {
                        first
                    } else {
                        word_start
                    };
                    if upper {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    first = false;
                    word_start = !c.is_alphanumeric();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "case_tests.rs"]
mod tests;
