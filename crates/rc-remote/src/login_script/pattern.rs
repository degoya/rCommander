//! [`Pattern`]: a `wait_for` regular expression, checked and compiled within limits.
//!
//! The `regex` crate matches in time linear in the text (no backtracking, no look-around or
//! back-references), so a pattern cannot stall a run however it is written; what it may cost is
//! bounded here instead: its length, and the size of the compiled program and of the lazy DFA.
//! The text it runs over is bounded by the engine's buffer ([`super::BUFFER_LIMIT`]).

use std::fmt;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Characters of a pattern, at most.
pub const MAX_PATTERN_CHARS: usize = 512;

/// Bytes the compiled program may take.
const COMPILED_LIMIT: usize = 256 * 1024;

/// Bytes the lazy DFA's cache may take.
const DFA_LIMIT: usize = 1024 * 1024;

/// Why a pattern is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("a pattern must not be empty")]
    Empty,
    #[error("a pattern holds at most {max} characters")]
    TooLong { max: usize },
    /// Not a regular expression, or one too large once compiled; `reason` is the regex
    /// crate's message, which quotes the pattern, never a secret.
    #[error("not a valid pattern: {reason}")]
    Syntax { reason: String },
}

/// A checked, compiled regular expression; equal to another when their sources are.
#[derive(Clone)]
pub struct Pattern {
    regex: Regex,
}

impl Pattern {
    /// `source` compiled within the limits.
    ///
    /// # Errors
    ///
    /// Empty, longer than [`MAX_PATTERN_CHARS`], not a regular expression, or too large.
    pub fn new(source: &str) -> Result<Self, PatternError> {
        if source.is_empty() {
            return Err(PatternError::Empty);
        }
        if source.chars().count() > MAX_PATTERN_CHARS {
            return Err(PatternError::TooLong {
                max: MAX_PATTERN_CHARS,
            });
        }
        let regex = RegexBuilder::new(source)
            .size_limit(COMPILED_LIMIT)
            .dfa_size_limit(DFA_LIMIT)
            .build()
            .map_err(|error| PatternError::Syntax {
                reason: error.to_string(),
            })?;
        Ok(Self { regex })
    }

    /// The pattern as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.regex.as_str()
    }

    /// Where the first match in `text` ends; `None` without one.
    #[must_use]
    pub fn match_end(&self, text: &str) -> Option<usize> {
        self.regex.find(text).map(|found| found.end())
    }
}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Pattern").field(&self.as_str()).finish()
    }
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Pattern {}

impl Serialize for Pattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Self::new(&source).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "pattern_tests.rs"]
mod tests;
