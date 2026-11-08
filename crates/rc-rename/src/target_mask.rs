//! Total Commander's wildcards in the target of a copy or a move (RC-110-04): `c:\target\*.bak`
//! copies every entry under its name with the extension `bak`.
//!
//! A mask is a different language from the multi-rename masks ([`crate::parse`]): it has no
//! placeholders, `[N]` is four characters. The mask and the name are split at their last dot and
//! each half is worked on its own (a mask without a dot works on the whole name, a name whose
//! only dot leads has no extension). Within a half a position walks the old name:
//!
//! - a literal character is written and moves the position one on;
//! - `?` copies the character at the position (nothing past the end) and moves on;
//! - `*` copies the rest from the position;
//! - `**` copies the whole half, whatever the position;
//! - `*?` copies nothing and sets the position back to the start.
//!
//! So `test.txt` with `h?.txt` is `he.txt`, with `F*Rest.*` `FestRest.txt`, with
//! `start_**_rest.*` `start_test_rest.txt`, and `name.txt` with `start_*?do*_rest.*`
//! `start_dome_rest.txt` - the examples of Total Commander's help.
//!
//! [`mask_targets`] applies a mask to the entries of one operation and refuses it before anything
//! is copied: two entries with one new name, a copy beside itself that keeps its name, a rename
//! onto the old name of another entry of the same rename (a chain, which the multi-rename tool
//! orders through temporary names). A name the file system cannot hold is the caller's check
//! (`check_plain`), as for every typed name.

use std::collections::HashMap;

use crate::{MAX_RULE_TEXT, RenameError};

/// Whether `text` holds a wildcard of a target mask, `*` or `?`.
#[must_use]
pub fn has_wildcards(text: &str) -> bool {
    text.contains(['*', '?'])
}

/// Refuses a mask longer than [`MAX_RULE_TEXT`] characters: masks are untrusted input (MCP).
///
/// # Errors
///
/// [`RenameError::TooLong`].
pub fn check_target_mask(mask: &str) -> Result<(), RenameError> {
    if mask.chars().count() > MAX_RULE_TEXT {
        return Err(RenameError::TooLong {
            field: "target mask",
            max: MAX_RULE_TEXT,
        });
    }
    Ok(())
}

/// The new name `mask` gives `name`.
#[must_use]
pub fn apply_target_mask(mask: &str, name: &str) -> String {
    let Some((stem_mask, extension_mask)) = mask.rsplit_once('.') else {
        return apply_half(mask, name);
    };
    let (stem, extension) = split_name(name);
    let mut new = apply_half(stem_mask, stem);
    let extension = apply_half(extension_mask, extension);
    if !extension.is_empty() {
        new.push('.');
        new.push_str(&extension);
    }
    new
}

/// `name` split at its last dot; a name whose only dot leads (`.profile`) has no extension.
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], &name[at + 1..]),
        _ => (name, ""),
    }
}

/// One half of a name, the stem or the extension, through its half of the mask.
fn apply_half(mask: &str, old: &str) -> String {
    let old: Vec<char> = old.chars().collect();
    let mut new = String::with_capacity(mask.len() + old.len());
    let mut position = 0;
    let mut chars = mask.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => match chars.peek() {
                Some('*') => {
                    chars.next();
                    new.extend(&old);
                    position = old.len();
                }
                Some('?') => {
                    chars.next();
                    position = 0;
                }
                _ => {
                    new.extend(old.iter().skip(position));
                    position = old.len();
                }
            },
            '?' => {
                if let Some(c) = old.get(position) {
                    new.push(*c);
                }
                position += 1;
            }
            literal => {
                new.push(literal);
                position += 1;
            }
        }
    }
    new
}

/// One entry of an operation with a target mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskSource<'a> {
    /// Its name.
    pub name: &'a str,
    /// It is copied or moved into the folder it is in (Shift+F5, a rename).
    pub same_folder: bool,
}

/// The new name of every entry of `sources` under `mask`, in their order; `None` for an entry
/// that is renamed in its own folder to the name it has, which is passed over. `moving` is a
/// move or a rename, `case_insensitive` a target folder that takes `A` and `a` for one name.
///
/// # Errors
///
/// [`RenameError::TooLong`] for a mask beyond [`MAX_RULE_TEXT`]; [`RenameError::MaskCollision`]
/// when two entries get one name; [`RenameError::MaskUnchanged`] for a copy into its own folder
/// that keeps its name, and when a rename changes no name at all; [`RenameError::MaskChain`] for
/// a rename onto the old name of another entry of the rename.
pub fn mask_targets(
    mask: &str,
    sources: &[MaskSource<'_>],
    moving: bool,
    case_insensitive: bool,
) -> Result<Vec<Option<String>>, RenameError> {
    check_target_mask(mask)?;
    let key = |name: &str| {
        if case_insensitive {
            name.to_lowercase()
        } else {
            name.to_owned()
        }
    };
    // The old names of the entries a rename leaves in their folder: a new name equal to one of
    // another entry's is a chain.
    let renamed_here: HashMap<String, usize> = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| moving && source.same_folder)
        .map(|(index, source)| (key(source.name), index))
        .collect();
    let mut taken: HashMap<String, usize> = HashMap::with_capacity(sources.len());
    let mut targets = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let new = apply_target_mask(mask, source.name);
        if source.same_folder && new == source.name {
            if !moving {
                return Err(RenameError::MaskUnchanged { name: new });
            }
            targets.push(None);
            continue;
        }
        let new_key = key(&new);
        if let Some(&first) = taken.get(&new_key) {
            return Err(RenameError::MaskCollision {
                first: sources[first].name.to_owned(),
                second: source.name.to_owned(),
                name: new,
            });
        }
        if let Some(&other) = renamed_here.get(&new_key)
            && other != index
        {
            return Err(RenameError::MaskChain {
                entry: source.name.to_owned(),
                name: new,
            });
        }
        taken.insert(new_key, index);
        targets.push(Some(new));
    }
    if let (true, Some(first)) = (targets.iter().all(Option::is_none), sources.first()) {
        return Err(RenameError::MaskUnchanged {
            name: first.name.to_owned(),
        });
    }
    Ok(targets)
}

#[cfg(test)]
#[path = "target_mask_tests.rs"]
mod tests;
