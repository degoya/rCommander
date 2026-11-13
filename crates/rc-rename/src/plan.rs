//! The preview: a rule applied to the entries of one directory, with the conflicts.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::case::{CaseMode, lower_small_words};
use crate::clock::{DEFAULT_DATE_FORMAT, DEFAULT_TIME_FORMAT};
use crate::error::{MaskField, RenameError};
use crate::names::{FsRules, InvalidName};
use crate::pattern::{Pattern, Token, parse};
use crate::render::{Fields, NoTimestamp, Switches, Times, from_root, path_levels, render};
use crate::rule::check_lengths;
pub use crate::rule::{
    CounterSettings, MAX_NAMES, MAX_NAMES_BYTES, MAX_RULE_TEXT, RenameContext, RenameRule,
    RenameSource, local_time,
};
use crate::split::split_name;
use crate::subdir::{check_path, normalized};

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
    /// The clipboard, the moments and the local formats the placeholders read.
    pub context: &'a RenameContext,
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
    /// The mask has a date placeholder and the entry has no modification time (or the time it
    /// reads lies outside the calendar after its offset).
    NoTimestamp,
    /// The rule's name list has no name for this entry.
    NamesMissing,
}

impl Conflict {
    /// Every code [`Conflict::code`] returns.
    pub const ALL_CODES: &[&str] = &[
        "rename_duplicate",
        "rename_target_exists",
        "rename_no_timestamp",
        "rename_names_missing",
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
            Self::NamesMissing => "rename_names_missing",
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

/// Where the new names come from: the masks, or the rule's name list.
enum NewNames<'a> {
    Masks { name: Pattern, extension: Pattern },
    List(&'a [String]),
}

/// Applies `rule` to the sources: the preview of the run, with every conflict.
///
/// The order per entry: the masks (or the entry's name from the rule's name list), the case of
/// the whole name, the search and replace, the name and the extension joined with a dot. A `\`
/// or `/` in a new name moves the entry into a subfolder; the new name then spells it `sub/name`
/// ([`crate::SUBFOLDER_SEPARATOR`]) and is held against [`check_path`].
///
/// # Errors
///
/// A rule text longer than [`MAX_RULE_TEXT`], a name list beyond its limits, a mask that does
/// not parse, `[X]` with an empty clipboard, a search that is not a valid regular expression.
/// Conflicts are not errors; they are in the items.
pub fn plan(input: &PlanInput<'_>, rule: &RenameRule) -> Result<RenamePlan, RenameError> {
    check_lengths(rule)?;
    let context = input.context;
    context.check()?;
    let new_names = match &rule.names {
        Some(names) => NewNames::List(names),
        None => NewNames::Masks {
            name: parse(&rule.name_mask).map_err(|error| RenameError::Mask {
                field: MaskField::Name,
                error,
            })?,
            extension: parse(&rule.extension_mask).map_err(|error| RenameError::Mask {
                field: MaskField::Extension,
                error,
            })?,
        },
    };
    let clipboard = match &new_names {
        NewNames::Masks { name, extension } => clipboard_text(context, [name, extension])?,
        NewNames::List(_) => String::new(),
    };
    let replacer = match &rule.search {
        Some(search) => search.compile()?,
        None => None,
    };
    let include_extension = rule
        .search
        .as_ref()
        .is_some_and(|search| search.include_extension);
    let levels = path_levels(input.dir);
    let from_root = from_root(input.dir);
    let count = u64::try_from(input.sources.len()).unwrap_or(u64::MAX);
    let fs = input.fs;

    let mut items = Vec::with_capacity(input.sources.len());
    for (index, source) in input.sources.iter().enumerate() {
        let (name, extension) = split_name(&source.name);
        let fields = Fields {
            name,
            extension,
            file_name: &source.name,
            levels: &levels,
            from_root: &from_root,
            size: source.size,
            is_dir: source.is_dir,
            index: u64::try_from(index).unwrap_or(u64::MAX),
            count,
            counter: rule.counter,
            times: Times {
                modified: source.modified,
                opened: context.opened_at,
                changed: context.changed_at,
            },
            clipboard: &clipboard,
            date_format: context
                .date_format
                .as_deref()
                .unwrap_or(DEFAULT_DATE_FORMAT),
            time_format: context
                .time_format
                .as_deref()
                .unwrap_or(DEFAULT_TIME_FORMAT),
        };
        let rendered = match &new_names {
            NewNames::Masks { name, extension } => {
                let mut switches = Switches::default();
                render(name, &fields, &mut switches)
                    .and_then(|name| {
                        render(extension, &fields, &mut switches).map(|extension| (name, extension))
                    })
                    .map_err(|NoTimestamp| Conflict::NoTimestamp)
            }
            NewNames::List(names) => match names.get(index) {
                Some(new) => {
                    let (name, extension) = split_name(new);
                    Ok((name.to_owned(), extension.to_owned()))
                }
                None => Err(Conflict::NamesMissing),
            },
        };
        let (name, extension) = match rendered {
            Ok(parts) => parts,
            Err(conflict) => {
                items.push(RenameItem {
                    old: source.name.clone(),
                    new: source.name.clone(),
                    change: Change::Unchanged,
                    conflict: Some(conflict),
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

/// The name and the extension in `case`, applied to them as one text `name.extension`.
fn cased(case: CaseMode, name: &str, extension: &str) -> (String, String) {
    let mut text = String::with_capacity(name.len() + extension.len() + 1);
    case.append(&mut text, name);
    let name_end = text.len();
    if extension.is_empty() {
        if case == CaseMode::Title {
            lower_small_words(&mut text, &[(0, name_end)]);
        }
        return (text, String::new());
    }
    case.append(&mut text, ".");
    case.append(&mut text, extension);
    if case == CaseMode::Title {
        let end = text.len();
        lower_small_words(&mut text, &[(0, end)]);
    }
    let extension = text.split_off(name_end + 1);
    text.truncate(name_end);
    (text, extension)
}

/// The clipboard's text as `[X]` inserts it: the line breaks at its end dropped, `*?:|"<>` as
/// `_` (Total Commander). Empty when no mask reads it.
///
/// # Errors
///
/// `[X]` in a mask with no text on the clipboard; a clipboard text longer than
/// [`MAX_RULE_TEXT`] that a mask reads.
fn clipboard_text(context: &RenameContext, masks: [&Pattern; 2]) -> Result<String, RenameError> {
    let mut used = false;
    let mut required = false;
    for token in masks.iter().flat_map(|mask| mask.tokens()) {
        if let Token::Clipboard { optional } = token {
            used = true;
            required |= !optional;
        }
    }
    if !used {
        return Ok(String::new());
    }
    let raw = context.clipboard.as_deref().unwrap_or_default();
    if raw.chars().count() > MAX_RULE_TEXT {
        return Err(RenameError::TooLong {
            field: "clipboard text",
            max: MAX_RULE_TEXT,
        });
    }
    let text: String = raw
        .trim_end_matches(['\r', '\n'])
        .chars()
        .map(|c| {
            if matches!(c, '*' | '?' | ':' | '|' | '"' | '<' | '>') {
                '_'
            } else {
                c
            }
        })
        .collect();
    if required && text.is_empty() {
        return Err(RenameError::ClipboardEmpty);
    }
    Ok(text)
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

#[cfg(test)]
#[path = "plan_extras_tests.rs"]
mod extras_tests;
