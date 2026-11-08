//! The case modes of the name and the extension.

use serde::{Deserialize, Serialize};

/// How the case of a rendered name or extension changes; Total Commander's six modes.
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
    /// `First Of Every Word Upper Case, English in Titles` (`[f]`): as `WordsUpper`, but the
    /// small words of English titles ([`SMALL_WORDS`]) lower case, unless one is the first or the
    /// last word.
    Title,
}

/// The words `[f]` leaves lower case inside a title, Total Commander's list.
pub const SMALL_WORDS: [&str; 20] = [
    "a", "an", "the", "at", "by", "for", "in", "of", "on", "to", "from", "up", "and", "as", "but",
    "or", "nor", "yet", "so", "vs",
];

impl CaseMode {
    /// `text` in this case.
    #[must_use]
    pub fn apply(self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        self.append(&mut out, text);
        if self == Self::Title {
            let end = out.len();
            lower_small_words(&mut out, &[(0, end)]);
        }
        out
    }

    /// Appends `text` to `out` in this case, as if the two were one text: a word (for
    /// `FirstUpper`, the text) starts only where `out` does not already continue one. This is
    /// how `[U]`, `[L]`, `[F]` and `[n]` in a mask apply to what follows them. `Title` appends
    /// as `WordsUpper` does; [`lower_small_words`] finishes it once the whole text is known.
    pub(crate) fn append(self, out: &mut String, text: &str) {
        match self {
            Self::Unchanged => out.push_str(text),
            Self::Lower => out.push_str(&text.to_lowercase()),
            Self::Upper => out.push_str(&text.to_uppercase()),
            Self::FirstUpper | Self::WordsUpper | Self::Title => {
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

/// Lower case for the small words of a title ([`SMALL_WORDS`]) that lie wholly in one of the
/// byte ranges `spans` (written in the `Title` mode) and are neither the first nor the last word
/// of `text`. A word is a run of letters and digits; the small words are ASCII, so the text keeps
/// its length.
pub(crate) fn lower_small_words(text: &mut str, spans: &[(usize, usize)]) {
    if spans.is_empty() {
        return;
    }
    let mut words = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                words.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push((from, text.len()));
    }
    // Spans written one after the other count as one.
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
    for &(start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let inside = |from: usize, to: usize| {
        merged
            .iter()
            .any(|&(start, end)| start <= from && to <= end)
    };
    let last = words.len().saturating_sub(1);
    for (index, &(from, to)) in words.iter().enumerate() {
        if index == 0 || index == last || !inside(from, to) {
            continue;
        }
        let small = SMALL_WORDS
            .iter()
            .any(|word| word.eq_ignore_ascii_case(&text[from..to]));
        if small {
            text[from..to].make_ascii_lowercase();
        }
    }
}

#[cfg(test)]
#[path = "case_tests.rs"]
mod tests;
