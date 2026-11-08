//! "Select duplicate files" (RC-100-98), as Total Commander's dialog behind Num+ over the result of
//! a duplicate search: select by mask and deselect by mask, by date or by size within each group,
//! or the files of chosen folders; then, when asked, leave at least one file of each group
//! unselected, the one three criteria in turn pick. Only the selection changes; deleting the
//! copies is F8's in the panel.
//!
//! The rules work on the panel's entries in list order, each with its group ([`Listed`]); a folder
//! is the part of an entry's name before its file name (the names of a flat view are relative to
//! the panel's folder).

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{mask::Mask, state::Selection};

/// What `panel.select_duplicates` does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateSelection {
    pub rule: DuplicateRule,
    /// "Leave at least one file in each group unselected": in a group whose files all end up
    /// selected, the one `keep` picks is deselected.
    #[serde(default)]
    pub leave_one: bool,
    /// The criteria that pick the file left, the first that tells one from the others deciding;
    /// the first file in the list when none does.
    #[serde(default = "default_keep")]
    pub keep: Vec<Keep>,
}

/// The selection rules of the dialog's two tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum DuplicateRule {
    /// The files whose name the mask matches (`*.jpg;*.png`) are selected - or, with `remove`,
    /// deselected.
    Mask {
        mask: String,
        #[serde(default)]
        remove: bool,
    },
    /// By the modification date within each group.
    Date { pick: DatePick },
    /// By the size within each group.
    Size { pick: SizePick },
    /// The files in the given folders and in those `pattern` matches (a Total Commander mask,
    /// matched against every folder name on the way, or only the direct parent with
    /// `parent_only`); with `subfolders` the files below them too. `remove` deselects them.
    Folders {
        #[serde(default)]
        folders: Vec<String>,
        #[serde(default)]
        pattern: Option<String>,
        #[serde(default)]
        parent_only: bool,
        #[serde(default)]
        subfolders: bool,
        #[serde(default)]
        remove: bool,
    },
}

/// "By date" in each group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatePick {
    /// Every file older than the newest.
    AllOlder,
    /// Every file newer than the oldest.
    AllNewer,
    /// Just the oldest file.
    Oldest,
    /// Just the newest file.
    Newest,
}

/// "By size" in each group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizePick {
    /// Every file larger than the smallest.
    AllLarger,
    /// Every file smaller than the largest.
    AllSmaller,
    /// Just the largest file.
    Largest,
    /// Just the smallest file.
    Smallest,
}

/// A criterion that picks the file left unselected in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keep {
    Newest,
    Oldest,
    Largest,
    Smallest,
    /// The first in the list.
    First,
    /// The last in the list.
    Last,
}

impl Keep {
    pub const ALL: [Self; 6] = [
        Self::Newest,
        Self::Oldest,
        Self::Largest,
        Self::Smallest,
        Self::First,
        Self::Last,
    ];
}

/// The criteria in force when none are given: date, then size, then the place in the list.
#[must_use]
pub fn default_keep() -> Vec<Keep> {
    vec![Keep::Newest, Keep::Largest, Keep::First]
}

/// One entry of a duplicate result, in list order.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Listed<'a> {
    pub(crate) name: &'a str,
    pub(crate) group: u32,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) size: u64,
}

/// The folders of a duplicate result's `names`, each once, sorted: the dialog's folder list.
pub fn duplicate_folders<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names
        .into_iter()
        .map(folder_of)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Whether `pattern` (a Total Commander mask) matches the folder `folder`: one of its names, or
/// with `parent_only` its last one only.
#[must_use]
pub fn folder_matches(folder: &str, pattern: &str, parent_only: bool) -> bool {
    let mask = Mask::parse(pattern).with_folders();
    let names: Vec<String> = Path::new(folder)
        .components()
        .filter_map(|part| match part {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parent_only {
        names.last().is_some_and(|name| mask.matches(name, true))
    } else {
        names.iter().any(|name| mask.matches(name, true))
    }
}

/// The selection after `spec` over the duplicate result `files`.
pub(crate) fn apply(spec: &DuplicateSelection, files: &[Listed<'_>], selection: &mut Selection) {
    match &spec.rule {
        DuplicateRule::Mask { mask, remove } => {
            let mask = Mask::parse(mask);
            for file in files {
                if mask.matches(file_name(file.name), false) {
                    mark(selection, file.name, !remove);
                }
            }
        }
        DuplicateRule::Date { pick } => {
            for group in groups(files).values() {
                for file in by_date(group, *pick) {
                    selection.insert(file.name.to_owned());
                }
            }
        }
        DuplicateRule::Size { pick } => {
            for group in groups(files).values() {
                for file in by_size(group, *pick) {
                    selection.insert(file.name.to_owned());
                }
            }
        }
        DuplicateRule::Folders {
            folders,
            pattern,
            parent_only,
            subfolders,
            remove,
        } => {
            let chosen: Vec<String> = duplicate_folders(files.iter().map(|file| file.name))
                .into_iter()
                .filter(|folder| {
                    folders.iter().any(|wanted| same_folder(folder, wanted))
                        || pattern
                            .as_deref()
                            .is_some_and(|pattern| folder_matches(folder, pattern, *parent_only))
                })
                .collect();
            for file in files {
                let folder = folder_of(file.name);
                let inside = chosen.iter().any(|chosen| {
                    same_folder(&folder, chosen) || (*subfolders && below(&folder, chosen))
                });
                if inside {
                    mark(selection, file.name, !remove);
                }
            }
        }
    }
    if spec.leave_one {
        leave_one(files, &spec.keep, selection);
    }
}

/// In each group whose files are all selected, the one `keep` picks is deselected.
fn leave_one(files: &[Listed<'_>], keep: &[Keep], selection: &mut Selection) {
    for group in groups(files).values() {
        if !group.iter().all(|file| selection.contains(file.name)) {
            continue;
        }
        let mut left: Vec<Listed<'_>> = group.clone();
        for criterion in keep {
            if left.len() <= 1 {
                break;
            }
            left = best(&left, *criterion);
        }
        if let Some(kept) = left.first() {
            selection.remove(kept.name);
        }
    }
}

/// The files of `group` that `criterion` likes best, in list order.
fn best<'a>(group: &[Listed<'a>], criterion: Keep) -> Vec<Listed<'a>> {
    let date = |file: &Listed<'_>| file.modified.unwrap_or(SystemTime::UNIX_EPOCH);
    let pick = |wanted: &dyn Fn(&Listed<'_>) -> bool| -> Vec<Listed<'a>> {
        group.iter().copied().filter(|file| wanted(file)).collect()
    };
    match criterion {
        Keep::Newest => match group.iter().map(date).max() {
            Some(newest) => pick(&|file| date(file) == newest),
            None => Vec::new(),
        },
        Keep::Oldest => match group.iter().map(date).min() {
            Some(oldest) => pick(&|file| date(file) == oldest),
            None => Vec::new(),
        },
        Keep::Largest => match group.iter().map(|file| file.size).max() {
            Some(largest) => pick(&|file| file.size == largest),
            None => Vec::new(),
        },
        Keep::Smallest => match group.iter().map(|file| file.size).min() {
            Some(smallest) => pick(&|file| file.size == smallest),
            None => Vec::new(),
        },
        Keep::First => group.first().copied().into_iter().collect(),
        Keep::Last => group.last().copied().into_iter().collect(),
    }
}

/// The files of `group` the date rule selects; "just" one is the first of equals in the list.
fn by_date<'a>(group: &[Listed<'a>], pick: DatePick) -> Vec<Listed<'a>> {
    let date = |file: &Listed<'_>| file.modified.unwrap_or(SystemTime::UNIX_EPOCH);
    let (Some(oldest), Some(newest)) = (group.iter().map(date).min(), group.iter().map(date).max())
    else {
        return Vec::new();
    };
    match pick {
        DatePick::AllOlder => group.iter().copied().filter(|f| date(f) < newest).collect(),
        DatePick::AllNewer => group.iter().copied().filter(|f| date(f) > oldest).collect(),
        DatePick::Oldest => best(group, Keep::Oldest).into_iter().take(1).collect(),
        DatePick::Newest => best(group, Keep::Newest).into_iter().take(1).collect(),
    }
}

/// The files of `group` the size rule selects; "just" one is the first of equals in the list.
fn by_size<'a>(group: &[Listed<'a>], pick: SizePick) -> Vec<Listed<'a>> {
    let size = |file: &Listed<'_>| file.size;
    let (Some(smallest), Some(largest)) =
        (group.iter().map(size).min(), group.iter().map(size).max())
    else {
        return Vec::new();
    };
    match pick {
        SizePick::AllLarger => group
            .iter()
            .copied()
            .filter(|f| f.size > smallest)
            .collect(),
        SizePick::AllSmaller => group.iter().copied().filter(|f| f.size < largest).collect(),
        SizePick::Largest => best(group, Keep::Largest).into_iter().take(1).collect(),
        SizePick::Smallest => best(group, Keep::Smallest).into_iter().take(1).collect(),
    }
}

/// The files by group, each group in list order.
fn groups<'a>(files: &[Listed<'a>]) -> BTreeMap<u32, Vec<Listed<'a>>> {
    let mut groups: BTreeMap<u32, Vec<Listed<'a>>> = BTreeMap::new();
    for file in files {
        groups.entry(file.group).or_default().push(*file);
    }
    groups
}

fn mark(selection: &mut Selection, name: &str, select: bool) {
    if select {
        selection.insert(name.to_owned());
    } else {
        selection.remove(name);
    }
}

/// The folder part of a flat name (`sub\file.txt` is in `sub`); empty for the panel's folder.
fn folder_of(name: &str) -> String {
    Path::new(name)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The file name of a flat name.
fn file_name(name: &str) -> &str {
    Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(name)
}

fn same_folder(folder: &str, other: &str) -> bool {
    Path::new(folder) == Path::new(other)
}

/// Whether `folder` lies below `above` (not `above` itself).
fn below(folder: &str, above: &str) -> bool {
    let (folder, above) = (Path::new(folder), Path::new(above));
    folder != above && folder.starts_with(above)
}

#[cfg(test)]
#[path = "select_duplicates_tests.rs"]
mod tests;
