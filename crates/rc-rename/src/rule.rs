//! What a plan is made from besides the folder: the rule the person set up, the entries, and the
//! context the window gives (clipboard, moments, local formats).

use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::case::CaseMode;
use crate::clock::MAX_FORMAT_TEXT;
use crate::error::RenameError;
use crate::replace::SearchReplace;

/// The most characters a mask, a search text or a replacement may have: rules are untrusted input (MCP), and no file name needs more.
pub const MAX_RULE_TEXT: usize = 1024;

/// The most names a name list ([`RenameRule::names`]) may have.
pub const MAX_NAMES: usize = 10_000;

/// The most bytes a name list may have, its names together (and a file it is read from).
pub const MAX_NAMES_BYTES: usize = 1 << 20;

/// What the person set up: masks, case, search and replace, counter - or a list of new names.
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
    /// Total Commander's "Load names from file": the n-th name is the n-th entry's new name,
    /// in place of the masks (case and search and replace still apply). An entry without a
    /// name has the conflict `names_missing`; names beyond the entries are not used.
    pub names: Option<Vec<String>>,
}

impl RenameRule {
    /// Refuses a rule with a text longer than [`MAX_RULE_TEXT`] characters, or a name list
    /// beyond [`MAX_NAMES`] and [`MAX_NAMES_BYTES`]; what [`crate::plan`] checks first, for a
    /// rule that is stored rather than planned (a preset).
    ///
    /// # Errors
    ///
    /// [`RenameError::TooLong`] naming the first text that is too long, or
    /// [`RenameError::NameList`].
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
            names: None,
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
    /// The size in bytes, for `[S]`.
    #[serde(default)]
    pub size: Option<u64>,
    /// The entry is a folder: `[S]` is 0, `[I]` takes its whole name.
    #[serde(default)]
    pub is_dir: bool,
}

impl RenameSource {
    /// A file of this name with no time and no size.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            modified: None,
            size: None,
            is_dir: false,
        }
    }
}

/// What a plan reads besides the rule and the entries, given by the window (or an agent): the
/// clipboard's text for `[X]`, the moments of `[T2]` and `[T3]`, the local formats of `[d]` and
/// `[t]`. The engine never reads the clipboard itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenameContext {
    /// The clipboard's text; `[X]` is an error without it.
    pub clipboard: Option<String>,
    /// When the dialog was opened, local (`[T2]`).
    pub opened_at: Option<NaiveDateTime>,
    /// When the options last changed, local (`[T3]`).
    pub changed_at: Option<NaiveDateTime>,
    /// The local date as a Windows picture (`dd.MM.yyyy`); ISO 8601 when `None`.
    pub date_format: Option<String>,
    /// The local time as a Windows picture (`HH:mm:ss`); 24 hours when `None`.
    pub time_format: Option<String>,
}

impl RenameContext {
    /// Refuses formats longer than [`MAX_FORMAT_TEXT`] characters.
    pub(crate) fn check(&self) -> Result<(), RenameError> {
        let formats = [
            ("date format", &self.date_format),
            ("time format", &self.time_format),
        ];
        match formats.into_iter().find(|(_, format)| {
            format
                .as_ref()
                .is_some_and(|format| format.chars().count() > MAX_FORMAT_TEXT)
        }) {
            Some((field, _)) => Err(RenameError::TooLong {
                field,
                max: MAX_FORMAT_TEXT,
            }),
            None => Ok(()),
        }
    }
}

/// A modification time as the date placeholders read it: local time.
#[must_use]
pub fn local_time(time: SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(time).naive_local()
}

/// Refuses a rule with a text longer than [`MAX_RULE_TEXT`] characters, or a name list that is
/// too large.
pub(crate) fn check_lengths(rule: &RenameRule) -> Result<(), RenameError> {
    let search = rule.search.as_ref();
    let texts = [
        ("name mask", Some(&rule.name_mask)),
        ("extension mask", Some(&rule.extension_mask)),
        ("search text", search.map(|search| &search.find)),
        ("replacement", search.map(|search| &search.replace)),
    ];
    if let Some((field, _)) = texts
        .into_iter()
        .find(|(_, text)| text.is_some_and(|text| text.chars().count() > MAX_RULE_TEXT))
    {
        return Err(RenameError::TooLong {
            field,
            max: MAX_RULE_TEXT,
        });
    }
    if let Some(names) = &rule.names {
        let bytes: usize = names.iter().map(String::len).sum();
        if names.len() > MAX_NAMES || bytes > MAX_NAMES_BYTES {
            return Err(RenameError::NameList {
                max_names: MAX_NAMES,
                max_bytes: MAX_NAMES_BYTES,
            });
        }
    }
    Ok(())
}
