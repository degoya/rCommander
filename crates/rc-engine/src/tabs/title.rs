//! A tab's title: the last part of its folder, as Total Commander shows it. Two tabs of a panel
//! with the same name get their parent's name added, `src (rc-db)` and `src (rc-vfs)`; what is
//! still equal then is numbered from the second on, `src (2)`.

use std::collections::HashMap;

use rc_vfs::VfsPath;

/// The titles of a panel's tabs, in their order.
#[must_use]
pub fn titles(paths: &[&VfsPath]) -> Vec<String> {
    let mut titles: Vec<String> = paths.iter().map(|path| last_part(path)).collect();
    for index in repeated(&titles) {
        if let Some(parent) = paths[index].parent() {
            titles[index] = format!("{} ({})", titles[index], last_part(&parent));
        }
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    for title in &mut titles {
        let count = seen.entry(title.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            *title = format!("{title} ({count})");
        }
    }
    titles
}

/// The folder's last part; a root, which has none, as it is written (`C:\`, `/`).
pub(crate) fn last_part(path: &VfsPath) -> String {
    path.path.file_name().map_or_else(
        || path.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The indices of titles that occur more than once.
fn repeated(titles: &[String]) -> Vec<usize> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for title in titles {
        *counts.entry(title.as_str()).or_insert(0) += 1;
    }
    (0..titles.len())
        .filter(|&index| counts[titles[index].as_str()] > 1)
        .collect()
}

#[cfg(test)]
#[path = "title_tests.rs"]
mod tests;
