//! Search and replace in the rendered name: plain text or a regular expression.

use regex::{Captures, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::error::RenameError;

/// The search and replace of a rule, applied after the masks and the case modes.
///
/// As in Total Commander, the search ignores case unless `case_sensitive` is set, replaces every
/// occurrence unless `first_only` is set and works on the name without the extension unless
/// `include_extension` is set. Plain text takes several terms separated by `|`
/// (`ae|oe` -> `a|o`): each is replaced by the replacement at its position, or by the whole
/// replacement when that has no `|`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchReplace {
    /// What to look for; nothing is replaced when it is empty.
    pub find: String,
    /// What to put in its place; with `regex`, `$1`, `${1}` and `${name}` insert groups and
    /// `$$` is a literal `$`.
    pub replace: String,
    /// `find` is a regular expression (the `regex` crate's syntax), not plain text.
    pub regex: bool,
    /// Upper and lower case are different letters (Total Commander's `^`).
    pub case_sensitive: bool,
    /// Replace only the first occurrence (`1x`).
    pub first_only: bool,
    /// Search in the whole name with the extension, not only in the name (`[E]`).
    pub include_extension: bool,
    /// When the search matches, the whole name becomes the replacement (`Subst.`).
    pub substitute: bool,
}

/// The longest compiled expression; a search for a file name never needs more.
const SIZE_LIMIT: usize = 1 << 20;

/// A [`SearchReplace`] ready to apply.
#[derive(Debug)]
pub(crate) struct Replacer {
    regex: Regex,
    replacement: Replacement,
    first_only: bool,
    substitute: bool,
}

#[derive(Debug)]
enum Replacement {
    /// A regular expression's replacement, with group references.
    Expand(String),
    /// The plain replacement of each term, by the term's group (1-based).
    Terms(Vec<String>),
}

impl SearchReplace {
    /// The compiled search, or `None` when there is nothing to search for.
    pub(crate) fn compile(&self) -> Result<Option<Replacer>, RenameError> {
        let (source, replacement) = if self.regex {
            if self.find.is_empty() {
                return Ok(None);
            }
            (self.find.clone(), Replacement::Expand(self.replace.clone()))
        } else {
            let terms: Vec<&str> = self
                .find
                .split('|')
                .filter(|term| !term.is_empty())
                .collect();
            if terms.is_empty() {
                return Ok(None);
            }
            let replacements: Vec<String> = if self.replace.contains('|') {
                let mut parts = self.replace.split('|');
                terms
                    .iter()
                    .map(|_| parts.next().unwrap_or_default().to_owned())
                    .collect()
            } else {
                vec![self.replace.clone(); terms.len()]
            };
            let alternatives: Vec<String> = terms
                .iter()
                .map(|term| format!("({})", regex::escape(term)))
                .collect();
            (alternatives.join("|"), Replacement::Terms(replacements))
        };
        let regex = RegexBuilder::new(&source)
            .case_insensitive(!self.case_sensitive)
            .size_limit(SIZE_LIMIT)
            .build()
            .map_err(|error| RenameError::Regex {
                message: error.to_string(),
            })?;
        Ok(Some(Replacer {
            regex,
            replacement,
            first_only: self.first_only,
            substitute: self.substitute,
        }))
    }
}

impl Replacer {
    /// `text` with the occurrences replaced, or the replacement alone when substituting.
    pub(crate) fn apply(&self, text: &str) -> String {
        if self.substitute {
            return match self.regex.captures(text) {
                Some(captures) => self.replacement_for(&captures),
                None => text.to_owned(),
            };
        }
        let limit = usize::from(self.first_only);
        self.regex
            .replacen(text, limit, |captures: &Captures<'_>| {
                self.replacement_for(captures)
            })
            .into_owned()
    }

    fn replacement_for(&self, captures: &Captures<'_>) -> String {
        match &self.replacement {
            Replacement::Expand(template) => {
                let mut out = String::new();
                captures.expand(template, &mut out);
                out
            }
            Replacement::Terms(replacements) => replacements
                .iter()
                .enumerate()
                .find(|(index, _)| captures.get(index + 1).is_some())
                .map(|(_, replacement)| replacement.clone())
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
#[path = "replace_tests.rs"]
mod tests;
