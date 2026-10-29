//! Name matching: Total Commander's masks, or a regular expression.
//!
//! A mask list is what Total Commander's "Search for" field takes: masks separated by `;` or
//! spaces (a mask with spaces in double quotes), `*` for any run of characters, `?` for one.
//! After a `|` come exclusions: a name matching one is never a hit, and an exclusion ending in
//! `\` or `/` (`.git\`) names directories the search does not enter at all. A mask without a
//! wildcard matches anywhere in the name (`report` is `*report*`; an exclusion is taken
//! exactly), and `*.*` also matches names without a dot, both as in Total Commander. An include
//! mask ending in `\` matches directories only. An empty list matches every name. The list is
//! read by the shared tokenizer, `rc_ops::mask::split_line`.

use regex::{Regex, RegexBuilder};

use crate::{
    EngineError,
    mask::{lowercase, split_line, wildcard},
};

/// A compiled name pattern.
#[derive(Debug)]
pub(crate) enum NameMatcher {
    Masks(MaskSet),
    Regex(Regex),
}

impl NameMatcher {
    /// Compiles `pattern`: a mask list, or a regular expression when `regex` is set.
    pub(crate) fn new(
        pattern: &str,
        regex: bool,
        case_sensitive: bool,
    ) -> Result<Self, EngineError> {
        if regex {
            RegexBuilder::new(pattern)
                .case_insensitive(!case_sensitive)
                .build()
                .map(Self::Regex)
                .map_err(|error| EngineError::InvalidSearch {
                    reason: error.to_string(),
                })
        } else {
            Ok(Self::Masks(MaskSet::parse(pattern, case_sensitive)))
        }
    }

    /// Whether `name` is a hit by name. A directory excluded by name is not a hit either.
    pub(crate) fn matches(&self, name: &str, is_dir: bool) -> bool {
        match self {
            Self::Regex(regex) => regex.is_match(name),
            Self::Masks(set) => set.matches(name, is_dir),
        }
    }

    /// Whether the search may enter the directory `name`.
    pub(crate) fn enters(&self, name: &str) -> bool {
        match self {
            Self::Regex(_) => true,
            Self::Masks(set) => set.enters(name),
        }
    }
}

/// Include masks, exclusions and directory exclusions.
#[derive(Debug, Default)]
pub(crate) struct MaskSet {
    /// Each mask with whether it is a directory mask.
    include: Vec<(Vec<char>, bool)>,
    exclude: Vec<Vec<char>>,
    dir_excludes: Vec<Vec<char>>,
    case_sensitive: bool,
}

impl MaskSet {
    fn parse(pattern: &str, case_sensitive: bool) -> Self {
        let (include, exclude) = split_line(pattern);
        let mut set = Self {
            case_sensitive,
            ..Self::default()
        };
        set.include = include
            .iter()
            .map(|token| (set.prepare(&token.text, true), token.dir))
            .collect();
        for token in exclude {
            let mask = set.prepare(&token.text, false);
            if token.dir {
                set.dir_excludes.push(mask);
            } else {
                set.exclude.push(mask);
            }
        }
        set
    }

    /// The mask as characters, lowered when case does not count; a `partial` one without a
    /// wildcard becomes `*mask*`.
    fn prepare(&self, mask: &str, partial: bool) -> Vec<char> {
        let mut chars: Vec<char> = if self.case_sensitive {
            mask.chars().collect()
        } else {
            lowercase(mask).collect()
        };
        if partial && !mask.contains(['*', '?']) {
            chars.insert(0, '*');
            chars.push('*');
        }
        chars
    }

    /// Whether `name` is a hit; the name is compared as the masks were prepared, without an
    /// allocation.
    fn matches(&self, name: &str, is_dir: bool) -> bool {
        if self.case_sensitive {
            self.hit(name.chars(), is_dir)
        } else {
            self.hit(lowercase(name), is_dir)
        }
    }

    /// Whether the search may enter the directory `name`.
    fn enters(&self, name: &str) -> bool {
        if self.case_sensitive {
            self.entered(name.chars())
        } else {
            self.entered(lowercase(name))
        }
    }

    fn hit<N: Iterator<Item = char> + Clone>(&self, name: N, is_dir: bool) -> bool {
        let any = |masks: &[Vec<char>]| masks.iter().any(|mask| fits(mask, name.clone()));
        let included = self.include.is_empty()
            || self
                .include
                .iter()
                .any(|(mask, dir)| (!dir || is_dir) && fits(mask, name.clone()));
        included && !any(&self.exclude) && !(is_dir && any(&self.dir_excludes))
    }

    fn entered<N: Iterator<Item = char> + Clone>(&self, name: N) -> bool {
        !self
            .dir_excludes
            .iter()
            .any(|mask| fits(mask, name.clone()))
    }
}

/// One mask against a prepared name; a mask ending in `.*` also takes a name without a dot.
fn fits<N: Iterator<Item = char> + Clone>(mask: &[char], name: N) -> bool {
    wildcard(mask, name.clone())
        || (mask.ends_with(&['.', '*'])
            && !name.clone().any(|c| c == '.')
            && wildcard(&mask[..mask.len() - 2], name))
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
