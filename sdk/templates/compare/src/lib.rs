//! A compare plugin: two `key=value` files (Java properties, `.env`) compared by key, whatever
//! the order of their lines. rCommander hands it the two files a person compares when both have
//! an extension its manifest claims, and shows what it answers beside the line diff.
//!
//! The logic is plain Rust over the shapes of `types.rs`; `src/guest.rs` reads the two files
//! through the host and translates the answer into the contract.

pub mod types;

#[cfg(target_arch = "wasm32")]
mod guest;

use std::collections::BTreeMap;

use types::{Change, ChangeKind, Comparison, Failure, FailureKind, Notice};

/// The most changes one answer lists; rCommander refuses more.
pub const MAX_CHANGES: usize = 10_000;

/// The code of a key found twice; `locales/en.json` has its text.
pub const DUPLICATE_KEY: &str = "{{PLUGIN_ID}}.duplicate_key";

/// One value of a file: its text and the line it is on, from 1.
struct Value<'a> {
    text: &'a str,
    line: u32,
}

/// The changes from `old` to `new`, both UTF-8 text.
///
/// # Errors
///
/// Either file is not UTF-8 text.
pub fn compare(old: &[u8], new: &[u8]) -> Result<Comparison, Failure> {
    let text = |bytes| std::str::from_utf8(bytes).map_err(|_| Failure::of(FailureKind::BadData));
    let mut notices = Vec::new();
    let old = values(text(old)?, "old", &mut notices);
    let new = values(text(new)?, "new", &mut notices);
    let mut comparison = Comparison {
        notices,
        ..Comparison::default()
    };
    let keys: std::collections::BTreeSet<&str> = old.keys().chain(new.keys()).copied().collect();
    for key in keys {
        let change = match (old.get(key), new.get(key)) {
            (Some(a), Some(b)) if a.text == b.text => continue,
            (Some(a), Some(b)) => change(ChangeKind::Changed, key, Some(a), Some(b)),
            (Some(a), None) => change(ChangeKind::Removed, key, Some(a), None),
            (None, Some(b)) => change(ChangeKind::Added, key, None, Some(b)),
            (None, None) => continue,
        };
        if comparison.changes.len() == MAX_CHANGES {
            comparison.truncated = true;
            break;
        }
        comparison.changes.push(change);
    }
    Ok(comparison)
}

fn change(kind: ChangeKind, key: &str, old: Option<&Value>, new: Option<&Value>) -> Change {
    Change {
        kind,
        path: key.to_owned(),
        column: None,
        old_value: old.map(|value| value.text.to_owned()),
        new_value: new.map(|value| value.text.to_owned()),
        old_line: old.map(|value| value.line),
        new_line: new.map(|value| value.line),
    }
}

/// Every `key=value` line of `text` by key; comments (`#`, `!`) and blank lines skipped. A key
/// found twice keeps its last value, and a notice says so.
fn values<'a>(
    text: &'a str,
    side: &str,
    notices: &mut Vec<Notice>,
) -> BTreeMap<&'a str, Value<'a>> {
    let mut values = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line_text = line.trim();
        if line_text.is_empty() || line_text.starts_with('#') || line_text.starts_with('!') {
            continue;
        }
        let (key, value) = line_text.split_once('=').unwrap_or((line_text, ""));
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let earlier = values.insert(
            key.trim(),
            Value {
                text: value.trim(),
                line: number,
            },
        );
        if earlier.is_some() {
            notices.push(Notice {
                code: DUPLICATE_KEY.to_owned(),
                params: vec![
                    ("key".to_owned(), key.trim().to_owned()),
                    ("side".to_owned(), side.to_owned()),
                ],
            });
        }
    }
    values
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
