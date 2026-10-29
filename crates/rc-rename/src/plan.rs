//! The preview: a rule applied to the entries of one directory, with the conflicts.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::case::CaseMode;
use crate::error::{MaskField, RenameError};
use crate::names::{FsRules, InvalidName};
use crate::pattern::parse;
use crate::render::{Fields, NoTimestamp, path_levels, render};
use crate::replace::SearchReplace;
use crate::subdir::{check_path, normalized};

/// The most characters a mask, a search text or a replacement may have: rules are untrusted input (MCP), and no file name needs more.
pub const MAX_RULE_TEXT: usize = 1024;

/// What the person set up: masks, case, search and replace, counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenameRule {
    /// The mask for the name without the extension; `[N]` keeps it.
    pub name_mask: String,
    /// The mask for the extension; `[E]` keeps it. When it renders nothing, the new name has
    /// no dot.
    pub extension_mask: String,
    /// The case of the whole rendered name, extension included (Total Commander's one
    /// drop-down).
    pub case: CaseMode,
    /// The search and replace after masks and case, if any.
    pub search: Option<SearchReplace>,
    /// The settings `[C]` uses where it does not set its own.
    pub counter: CounterSettings,
}

impl RenameRule {
    /// Refuses a rule with a text longer than [`MAX_RULE_TEXT`] characters; what [`plan`]
    /// checks first, for a rule that is stored rather than planned (a preset).
    ///
    /// # Errors
    ///
    /// [`RenameError::TooLong`] naming the first text that is too long.
    pub fn check(&self) -> Result<(), RenameError> {
        check_lengths(self)
    }
}

impl Default for RenameRule {
    fn default() -> Self {
        Self {
            name_mask: "[N]".to_owned(),
            extension_mask: "[E]".to_owned(),
            case: CaseMode::Unchanged,
            search: None,
            counter: CounterSettings::default(),
        }
    }
}

/// The counter: the first entry gets `start`, each further one `step` more, at least `width`
/// digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CounterSettings {
    pub start: i64,
    pub step: i64,
    pub width: u32,
}

impl Default for CounterSettings {
    fn default() -> Self {
        Self {
            start: 1,
            step: 1,
            width: 1,
        }
    }
}

/// One entry to rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameSource {
    /// The current name, without the directory.
    pub name: String,
    /// The local modification time, for the date placeholders.
    #[serde(default)]
    pub modified: Option<NaiveDateTime>,
}

/// A modification time as the date placeholders read it: local time.
#[must_use]
pub fn local_time(time: SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(time).naive_local()
}

/// What a plan is made from.
#[derive(Debug, Clone, Copy)]
pub struct PlanInput<'a> {
    /// The directory the entries are in, as the backend spells it (for `[A]`, `[P]` and the path
    /// length).
    pub dir: &'a str,
    /// The entries to rename, in the order the counter counts them.
    pub sources: &'a [RenameSource],
    /// Every name in the directory, the sources included or not.
    pub existing: &'a [String],
    /// The naming rules of the directory's file system.
    pub fs: FsRules,
}

/// The kind of change for one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// The new name is the old one.
    Unchanged,
    /// A new name.
    Rename,
    /// Only upper and lower case change, on a file system that ignores case: carried out through
    /// a temporary name, never a conflict.
    CaseOnly,
}

/// Why an entry cannot be renamed as planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Conflict {
    /// Another entry of the plan gets the same new name.
    Duplicate {
        /// The index of (the first) other entry.
        other: usize,
    },
    /// The new name is taken by an entry that keeps its name.
    TargetExists,
    /// The file system refuses the new name.
    InvalidName { reason: InvalidName },
    /// The mask has a date placeholder and the entry has no modification time.
    NoTimestamp,
}

impl Conflict {
    /// Every code [`Conflict::code`] returns.
    pub const ALL_CODES: &[&str] = &[
        "rename_duplicate",
        "rename_target_exists",
        "rename_no_timestamp",
    ];

    /// The stable code; its message is `error.<code>` in the i18n catalogue. An invalid name
    /// has the code of its [`InvalidName`].
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Duplicate { .. } => "rename_duplicate",
            Self::TargetExists => "rename_target_exists",
            Self::InvalidName { reason } => reason.code(),
            Self::NoTimestamp => "rename_no_timestamp",
        }
    }
}

/// One line of the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameItem {
    /// The current name.
    pub old: String,
    /// The name after the rename; the old one where the entry keeps it.
    pub new: String,
    /// What kind of change this is.
    pub change: Change,
    /// What stops it, if anything; the first by the order invalid name, duplicate, taken.
    pub conflict: Option<Conflict>,
}

/// The preview of a run: one item per source, in the sources' order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenamePlan {
    /// The directory.
    pub dir: String,
    /// The file system's naming rules the plan was checked against.
    pub fs: FsRules,
    /// One per source.
    pub items: Vec<RenameItem>,
    /// The keys ([`FsRules::key`]) of every name in the directory before the run. Not part of
    /// the serde form: a plan read back is for showing, a plan to schedule is made anew.
    #[serde(skip)]
    pub(crate) taken: HashSet<String>,
}

impl RenamePlan {
    /// How many items have a conflict.
    #[must_use]
    pub fn conflict_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.conflict.is_some())
            .count()
    }

    /// Whether any item has a conflict; such a plan is not carried out.
    #[must_use]
    pub fn has_conflicts(&self) -> bool {
        self.conflict_count() > 0
    }
}

/// Applies `rule` to the sources: the preview of the run, with every conflict.
///
/// The order per entry: the masks, the case of the whole name, the search and replace, the name
/// and the extension joined with a dot. A `\` or `/` in a new name moves the entry into a
/// subfolder; the new name then spells it `sub/name` ([`crate::SUBFOLDER_SEPARATOR`]) and is
/// held against [`check_path`].
///
/// # Errors
///
/// A rule text longer than [`MAX_RULE_TEXT`], a mask that does not parse, a search that is not
/// a valid regular expression. Conflicts are not errors; they are in the items.
pub fn plan(input: &PlanInput<'_>, rule: &RenameRule) -> Result<RenamePlan, RenameError> {
    check_lengths(rule)?;
    let name_mask = parse(&rule.name_mask).map_err(|error| RenameError::Mask {
        field: MaskField::Name,
        error,
    })?;
    let extension_mask = parse(&rule.extension_mask).map_err(|error| RenameError::Mask {
        field: MaskField::Extension,
        error,
    })?;
    let replacer = match &rule.search {
        Some(search) => search.compile()?,
        None => None,
    };
    let include_extension = rule
        .search
        .as_ref()
        .is_some_and(|search| search.include_extension);
    let levels = path_levels(input.dir);
    let parent = levels.last().copied().unwrap_or_default();
    let grandparent = levels
        .len()
        .checked_sub(2)
        .and_then(|index| levels.get(index))
        .copied()
        .unwrap_or_default();
    let count = u64::try_from(input.sources.len()).unwrap_or(u64::MAX);
    let fs = input.fs;

    let mut items = Vec::with_capacity(input.sources.len());
    for (index, source) in input.sources.iter().enumerate() {
        let (name, extension) = split_name(&source.name);
        let fields = Fields {
            name,
            extension,
            file_name: &source.name,
            parent,
            grandparent,
            index: u64::try_from(index).unwrap_or(u64::MAX),
            count,
            counter: rule.counter,
            modified: source.modified,
        };
        let rendered = render(&name_mask, &fields)
            .and_then(|name| render(&extension_mask, &fields).map(|extension| (name, extension)));
        let (name, extension) = match rendered {
            Ok(parts) => parts,
            Err(NoTimestamp) => {
                items.push(RenameItem {
                    old: source.name.clone(),
                    new: source.name.clone(),
                    change: Change::Unchanged,
                    conflict: Some(Conflict::NoTimestamp),
                });
                continue;
            }
        };
        let (mut name, extension) = cased(rule.case, &name, &extension);
        let new = match &replacer {
            Some(replacer) if include_extension => {
                replacer.apply(&join_extension(&name, &extension))
            }
            Some(replacer) => {
                name = replacer.apply(&name);
                join_extension(&name, &extension)
            }
            None => join_extension(&name, &extension),
        };
        let new = if new == source.name {
            new
        } else {
            normalized(&new)
        };
        let change = if new == source.name {
            Change::Unchanged
        } else if fs.case_insensitive && fs.key(&new) == fs.key(&source.name) {
            Change::CaseOnly
        } else {
            Change::Rename
        };
        items.push(RenameItem {
            old: source.name.clone(),
            new,
            change,
            conflict: None,
        });
    }

    let taken: HashSet<String> = input
        .existing
        .iter()
        .chain(input.sources.iter().map(|source| &source.name))
        .map(|name| fs.key(name))
        .collect();
    mark_conflicts(&mut items, input.dir, &taken, &fs);
    Ok(RenamePlan {
        dir: input.dir.to_owned(),
        fs,
        items,
        taken,
    })
}

/// Sets the conflict of every item that has one: invalid names first, then duplicates, then
/// names taken by entries that keep theirs.
fn mark_conflicts(items: &mut [RenameItem], dir: &str, taken: &HashSet<String>, fs: &FsRules) {
    let moving_away: HashSet<String> = items
        .iter()
        .filter(|item| item.change == Change::Rename)
        .map(|item| fs.key(&item.old))
        .collect();
    let mut by_target: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if item.change != Change::Unchanged {
            by_target.entry(fs.key(&item.new)).or_default().push(index);
        }
    }
    for (index, item) in items.iter_mut().enumerate() {
        if item.change == Change::Unchanged || item.conflict.is_some() {
            continue;
        }
        let key = fs.key(&item.new);
        item.conflict = if let Some(reason) = check_path(&item.new, dir, fs) {
            Some(Conflict::InvalidName { reason })
        } else if let Some(&other) = by_target
            .get(&key)
            .and_then(|group| group.iter().find(|&&other| other != index))
        {
            Some(Conflict::Duplicate { other })
        } else if item.change == Change::Rename
            && taken.contains(&key)
            && !moving_away.contains(&key)
        {
            Some(Conflict::TargetExists)
        } else {
            None
        };
    }
}

/// Refuses a rule with a text longer than [`MAX_RULE_TEXT`] characters.
fn check_lengths(rule: &RenameRule) -> Result<(), RenameError> {
    let search = rule.search.as_ref();
    let texts = [
        ("name mask", Some(&rule.name_mask)),
        ("extension mask", Some(&rule.extension_mask)),
        ("search text", search.map(|search| &search.find)),
        ("replacement", search.map(|search| &search.replace)),
    ];
    match texts
        .into_iter()
        .find(|(_, text)| text.is_some_and(|text| text.chars().count() > MAX_RULE_TEXT))
    {
        Some((field, _)) => Err(RenameError::TooLong {
            field,
            max: MAX_RULE_TEXT,
        }),
        None => Ok(()),
    }
}

/// The name and the extension: split at the last dot, unless the dot starts the name
/// (`.bashrc`) or ends it (`notes.`). Folders too, as in Total Commander.
pub(crate) fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// The name and the extension in `case`, applied to them as one text `name.extension`.
fn cased(case: CaseMode, name: &str, extension: &str) -> (String, String) {
    let mut text = String::with_capacity(name.len() + extension.len() + 1);
    case.append(&mut text, name);
    let name_end = text.len();
    if extension.is_empty() {
        return (text, String::new());
    }
    case.append(&mut text, ".");
    case.append(&mut text, extension);
    let extension = text.split_off(name_end + 1);
    text.truncate(name_end);
    (text, extension)
}

fn join_extension(name: &str, extension: &str) -> String {
    if extension.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{extension}")
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
