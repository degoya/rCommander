//! The one wildcard matcher of the workspace and Total Commander's selection masks built on it.
//!
//! [`wildcard`] is `*` (any run of characters) and `?` (one character) against a whole name; every
//! mask syntax in rCommander - the panels' Num+/Num- masks, the unpack mask, the search's and the
//! synchronize dialog's mask lists, the quick search and filter - compares names through it.
//!
//! [`Mask`] is a mask line as Num+, Num- and the unpack dialog take it: several masks separated
//! by `;`, masks after a `|` excluded (`*.txt;*.md|read*`), case ignored. A mask ending in `\` or
//! `/` matches directories (`*\`), any other one files; `*.*` matches every file, with or without
//! a dot. [`Mask::with_folders`] lets a line without a directory mask match directories by the
//! same names (folders in the selection, RC-020-08). [`lowercase`] is how a name is lowered for
//! the comparison without an allocation. `rc-engine` re-exports all three from its `mask` module.
//!
//! [`split_line`] and [`tokens`] are the one reader of Total Commander's mask lists that every
//! mask syntax shares (RC-080-48): masks separated by `;` or white space, a mask with spaces in
//! double quotes, exclusions after the first `|`, a trailing `\` or `/` marking a directory mask,
//! `*.*` read as `*`. What a mask then means - a whole name, part of it, directories or files -
//! is the caller's.

use std::borrow::Borrow;

/// A parsed mask line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    include: Vec<Part>,
    exclude: Vec<Part>,
    /// Every part matches directories as well as files ([`Mask::with_folders`]).
    folders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Part {
    /// The wildcard pattern in lower case.
    pattern: Vec<char>,
    /// Whether it matches directories instead of files.
    dirs: bool,
}

impl Mask {
    /// The mask line `text`. Nothing before the `|` but something after it means every file;
    /// an empty line matches nothing.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let (include, exclude) = split_line(text);
        let mut include = parts(include);
        let exclude = parts(exclude);
        if include.is_empty() && !exclude.is_empty() {
            include = vec![every_file()];
        }
        Self {
            include,
            exclude,
            folders: false,
        }
    }

    /// Like [`Mask::parse`], but nothing before the `|` always means every file, so an empty
    /// line matches every file too (the unpack dialog's mask).
    #[must_use]
    pub fn parse_or_all(text: &str) -> Self {
        let mut mask = Self::parse(text);
        if mask.include.is_empty() {
            mask.include = vec![every_file()];
        }
        mask
    }

    /// The same mask matching directories by the same names too (`*.*` every directory),
    /// exclusions included - unless the line has a directory mask (`*\`), which keeps the line
    /// as it is.
    #[must_use]
    pub fn with_folders(mut self) -> Self {
        self.folders = !self
            .include
            .iter()
            .chain(&self.exclude)
            .any(|part| part.dirs);
        self
    }

    /// Whether the entry `name`, a directory when `dir`, is matched.
    #[must_use]
    pub fn matches(&self, name: &str, dir: bool) -> bool {
        let name = lowercase(name);
        let hit = |part: &Part| {
            (self.folders || part.dirs == dir) && wildcard(&part.pattern, name.clone())
        };
        self.include.iter().any(hit) && !self.exclude.iter().any(hit)
    }
}

fn parts(tokens: Vec<Token>) -> Vec<Part> {
    tokens
        .into_iter()
        .map(|token| Part {
            pattern: lowercase(&token.text).collect(),
            dirs: token.dir,
        })
        .collect()
}

fn every_file() -> Part {
    Part {
        pattern: vec!['*'],
        dirs: false,
    }
}

/// One mask of a mask list, as [`tokens`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The mask as written, without its directory mark and its quotes; `*.*` and a bare
    /// directory mark (`\`) are `*`.
    pub text: String,
    /// Whether it ended in `\` or `/`: a directory mask.
    pub dir: bool,
}

/// A mask line split at its first `|`: the masks and the exclusions, each read by [`tokens`].
#[must_use]
pub fn split_line(line: &str) -> (Vec<Token>, Vec<Token>) {
    let (include, exclude) = line.split_once('|').unwrap_or((line, ""));
    (tokens(include), tokens(exclude))
}

/// The masks of one side of `|`: separated by `;` or white space outside double quotes, which
/// keep a mask with spaces (or a `;`) together; empty masks are dropped.
#[must_use]
pub fn tokens(list: &str) -> Vec<Token> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in list.chars() {
        match c {
            '"' => quoted = !quoted,
            c if !quoted && (c == ';' || c.is_whitespace()) => {
                words.push(std::mem::take(&mut current));
            }
            c => current.push(c),
        }
    }
    words.push(current);
    words
        .into_iter()
        .filter(|word| !word.is_empty())
        .map(|word| {
            let dir = word.ends_with(['\\', '/']);
            let text = word.trim_end_matches(['\\', '/']);
            let text = if text == "*.*" || text.is_empty() {
                "*"
            } else {
                text
            };
            Token {
                text: text.to_owned(),
                dir,
            }
        })
        .collect()
}

/// `text` in lower case, character by character and without an allocation: what every mask
/// compares a name through when case is ignored. It differs from `str::to_lowercase` only in the
/// Greek final sigma, which a file mask can live with.
pub fn lowercase(text: &str) -> impl Iterator<Item = char> + Clone + '_ {
    text.chars().flat_map(char::to_lowercase)
}

/// Whether `name` fits `pattern`, `*` standing for any run of characters and `?` for one, both
/// compared as given (the caller lowers them to ignore case, [`lowercase`] without allocating).
/// The name is anything that iterates over characters and whose iterator clones - a slice of
/// them, `str::chars`, [`lowercase`] - so no caller collects it. Iterative with one backtrack
/// point, the last `*`: linear for ordinary masks and never recursive.
#[must_use]
pub fn wildcard<N>(pattern: &[char], name: N) -> bool
where
    N: IntoIterator,
    N::Item: Borrow<char>,
    N::IntoIter: Clone,
{
    let mut rest = name.into_iter();
    let mut p = 0;
    // The last `*` and the name position it was tried at, to come back to.
    let mut star: Option<(usize, N::IntoIter)> = None;
    loop {
        while pattern.get(p) == Some(&'*') {
            star = Some((p, rest.clone()));
            p += 1;
        }
        let Some(c) = rest.next() else { break };
        match pattern.get(p) {
            Some(&m) if m == '?' || m == *c.borrow() => p += 1,
            _ => match &mut star {
                Some((at, from)) => {
                    // The `*` takes one more character; the rest is tried again after it.
                    p = *at + 1;
                    from.next();
                    rest = from.clone();
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
