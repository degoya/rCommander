//! A name split at its last dot (RC-110-30): the two rules the workspace has, each under its own
//! name. They differ only for a name that ends in a dot.
//!
//! | name       | [`split_name`]      | [`split_extension`] |
//! | ---------- | ------------------- | ------------------- |
//! | `notes.`   | `("notes.", "")`    | `("notes", "")`     |
//! | `.bashrc`  | `(".bashrc", "")`   | `(".bashrc", "")`   |
//! | `a.tar.gz` | `("a.tar", "gz")`   | `("a.tar", "gz")`   |
//! | `noext`    | `("noext", "")`     | `("noext", "")`     |

/// A name split at its last dot into name and extension (without the dot), as Total Commander
/// shows and renames it: `notes.txt` is `("notes", "txt")`, `a.tar.gz` is `("a.tar", "gz")`. A
/// dot that starts the name (`.bashrc`) or ends it (`notes.`) splits nothing: `("notes.", "")`,
/// so name and extension joined by a dot - or the name alone, without an extension - give the
/// name back. A folder's name is split like a file's.
#[must_use]
pub fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// A name split at its last dot wherever that leaves a stem: only a dot that starts the name
/// (`.bashrc`) splits nothing. A dot that ends it is cut off - `notes.` is `("notes", "")` -
/// which is what a copy mask, a split file's part names and a command's `%N`/`%E` go by.
#[must_use]
pub fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;
