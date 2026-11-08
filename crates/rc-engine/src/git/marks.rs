//! git's paths as marks of one folder's entries (RC-100-97). git names paths relative to the
//! repository's root; the panel shows one folder, `prefix` below the root. A path in that
//! folder marks its entry; a path deeper down gives the sub-folder it lies in a dot, in the
//! colour of the most important change below it (conflict, modified, added, renamed,
//! untracked); something ignored or deleted deeper down gives no dot of its own colour - a
//! deletion counts as a change. A file deleted in the folder itself is only counted. When git
//! names the folder itself, or one above it, whole (untracked, ignored), every entry has that mark.

use std::collections::BTreeMap;

use super::{Change, GitMark, Item};

/// One folder's marks: each entry's by name, the mark of all of them, the files deleted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FolderMarks {
    pub marks: BTreeMap<String, GitMark>,
    pub whole: Option<GitMark>,
    pub deleted: usize,
}

/// The marks of the entries of the folder `prefix` (relative to the root, `/`-separated, empty
/// for the root), from `items`; ignored entries only with `ignored`. With `fold_case` names
/// are compared, and keyed, in lower case.
pub(crate) fn summarize(
    items: &[Item],
    prefix: &str,
    ignored: bool,
    fold_case: bool,
) -> FolderMarks {
    let fold = |text: &str| {
        if fold_case {
            text.to_lowercase()
        } else {
            text.to_owned()
        }
    };
    let prefix = fold(prefix.trim_matches('/'));
    let mut summary = FolderMarks::default();
    for item in items {
        let path = fold(&item.path);
        let change = match item.change {
            Change::Mark(GitMark::Ignored) if !ignored => continue,
            change => change,
        };
        // The folder shown, or one above it, named whole.
        if let Some(folder) = path.strip_suffix('/')
            && within(&prefix, folder)
        {
            if let Change::Mark(mark) = change {
                summary.whole = summary.whole.max(Some(mark));
            }
            continue;
        }
        let Some(rest) = below(&path, &prefix) else {
            continue;
        };
        let (name, deeper) = match rest.split_once('/') {
            // `name/` alone: a folder git names whole.
            Some((name, tail)) => (name, !tail.is_empty()),
            None => (rest, false),
        };
        if name.is_empty() {
            continue;
        }
        let mark = match (change, deeper) {
            (Change::Deleted, false) => {
                summary.deleted += 1;
                continue;
            }
            (Change::Mark(GitMark::Ignored), true) => continue,
            (Change::Deleted, true) => GitMark::Modified,
            (Change::Mark(mark), _) => mark,
        };
        let entry = summary.marks.entry(name.to_owned()).or_insert(mark);
        *entry = (*entry).max(mark);
    }
    summary
}

/// Whether the folder `prefix` is `folder` or lies below it.
fn within(prefix: &str, folder: &str) -> bool {
    prefix == folder
        || folder.is_empty()
        || prefix
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// `path` relative to the folder `prefix`; `None` when it lies elsewhere.
fn below<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        return Some(path);
    }
    path.strip_prefix(prefix)?.strip_prefix('/')
}

#[cfg(test)]
#[path = "marks_tests.rs"]
mod tests;
