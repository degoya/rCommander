//! The multi-rename engine: everything about renaming many files at once that needs no file
//! system.
//!
//! A [`RenameRule`] holds what the person set up - a name and an extension mask with Total
//! Commander's placeholders ([`parse`]), the case of the whole name ([`CaseMode`]), an optional
//! search and replace ([`SearchReplace`]) and the counter, or a list of new names
//! ([`names_from_bytes`]). A [`RenameContext`] brings what the window knows: the clipboard's text,
//! the moments of `[T2]` and `[T3]`, the local date and time formats. [`plan`] applies it to the entries of
//! one directory and returns a [`RenamePlan`]: per entry `old -> new`, what kind of change that
//! is, and the conflict that stops it (two entries with one new name, a name that is taken, a
//! name the file system refuses). [`schedule`] turns a plan without conflicts into
//! [`RenameStep`]s in an order in which no rename overwrites another entry, swaps and rings
//! through temporary names; [`undo_steps`] and [`check_undo`] reverse a run, or the part of it
//! that was carried out.
//!
//! A `\` in a new name moves the entry into a subfolder of its directory, never above it
//! ([`check_path`]); a plan spells such a name `sub/name` ([`SUBFOLDER_SEPARATOR`]).
//!
//! Every new name is checked against the Windows naming rules on every file system, and rule
//! texts are bounded ([`MAX_RULE_TEXT`]): rules are untrusted input. [`UndoRecord`] is what the
//! undo log keeps per run, within [`UNDO_LOG_MAX_STEPS`] and [`UNDO_LOG_MAX_AGE_DAYS`];
//! [`applied_count`] reads how far a logged run got, also after a crash.
//!
//! The steps are carried out as a job of the operations queue (`rc-ops`, `JobKind::Rename`)
//! against any `Vfs`; the undo log is the table `rename_log` in `rc-db`; the `multi_rename.*`
//! commands (`rc-engine`) and their MCP tools put the two together.
//!
//! A copy or move target with Total Commander's wildcards (`target\*.bak`, RC-110-04) is the
//! other mask language here: [`apply_target_mask`] and [`mask_targets`].

mod args;
mod case;
mod chars;
mod clock;
mod counter;
mod error;
mod name_list;
mod names;
mod pattern;
mod plan;
mod render;
mod replace;
mod rule;
mod schedule;
mod subdir;
mod target_mask;
mod undo_log;

pub use case::{CaseMode, SMALL_WORDS};
pub use chars::{invisible, turns_direction};
pub use clock::{
    DEFAULT_DATE_FORMAT, DEFAULT_TIME_FORMAT, MAX_FORMAT_TEXT, OffsetUnit, TimeOffset, TimeSource,
    TimeSwitch,
};
pub use error::{MaskField, PatternError, PatternErrorKind, RenameError};
pub use name_list::{names_from_bytes, names_from_text, names_to_bytes};
pub use names::{FsRules, InvalidName, NameRules, check_name, check_plain};
pub use pattern::{
    CounterSpec, CounterWidth, DatePart, LetterCounter, Pattern, Range, Token, parse,
};
pub use plan::{
    Change, Conflict, CounterSettings, MAX_NAMES, MAX_NAMES_BYTES, MAX_RULE_TEXT, PlanInput,
    RenameContext, RenameItem, RenamePlan, RenameRule, RenameSource, local_time, plan,
};
pub use replace::SearchReplace;
pub use schedule::{
    RenameStep, Schedule, TEMP_PREFIX, UndoMismatch, UndoProblem, check_undo, schedule, undo_steps,
};
pub use subdir::{SUBFOLDER_SEPARATOR, check_path, nested_path};
pub use target_mask::{
    MaskSource, apply_target_mask, check_target_mask, has_wildcards, mask_targets,
};
pub use undo_log::{UNDO_LOG_MAX_AGE_DAYS, UNDO_LOG_MAX_STEPS, UndoRecord, applied_count};
