//! File masks as Total Commander writes them: `*.rs *.toml | *.bak target\`.
//!
//! Masks are separated by spaces or `;` (a name with spaces goes in double quotes); what follows
//! the first `|` excludes. `*` stands for any run of characters, `?` for one; matching ignores
//! case and looks at the name only. A mask ending in `\` or `/` is a directory mask: it excludes
//! directories (with everything in them), and the other masks apply to files. `*.*` matches every
//! file, with or without an extension. The list is read by the shared tokenizer,
//! `rc_ops::mask::split_line`.

use crate::mask::{Token, lowercase, split_line, wildcard};

/// Parsed include and exclude masks. The empty mask includes everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Masks {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pattern {
    /// Lower case, without the directory mark.
    glob: Vec<char>,
    dir: bool,
}

impl Masks {
    /// Reads `spec`; every string is a valid mask.
    #[must_use]
    pub fn parse(spec: &str) -> Self {
        let (include, exclude) = split_line(spec);
        Self {
            include: patterns(include),
            exclude: patterns(exclude),
        }
    }

    /// Whether a file of this name takes part: an include mask matches it (or there is none)
    /// and no exclude mask does.
    #[must_use]
    pub fn includes_file(&self, name: &str) -> bool {
        let name = lower(name);
        let included = file_masks(&self.include).next().is_none()
            || file_masks(&self.include).any(|pattern| pattern.matches(&name));
        included && !file_masks(&self.exclude).any(|pattern| pattern.matches(&name))
    }

    /// Whether a directory of this name takes part: no exclude directory mask matches it.
    #[must_use]
    pub fn includes_dir(&self, name: &str) -> bool {
        let name = lower(name);
        !self
            .exclude
            .iter()
            .any(|pattern| pattern.dir && pattern.matches(&name))
    }
}

fn file_masks(patterns: &[Pattern]) -> impl Iterator<Item = &Pattern> {
    patterns.iter().filter(|pattern| !pattern.dir)
}

fn lower(name: &str) -> Vec<char> {
    lowercase(name).collect()
}

fn patterns(tokens: Vec<Token>) -> Vec<Pattern> {
    tokens
        .into_iter()
        .map(|token| Pattern {
            glob: lower(&token.text),
            dir: token.dir,
        })
        .collect()
}

impl Pattern {
    fn matches(&self, name: &[char]) -> bool {
        wildcard(&self.glob, name)
    }
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
