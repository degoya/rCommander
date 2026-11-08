//! The native side of the `compare-plugin` world (RC-100-41): the records of `interface compare`
//! and the [`Compare`] trait a structural comparison's logic implements against a
//! [`CompareHost`].
//!
//! A compare plugin reads two files of the kinds its manifest claims (`[match]`) and answers what
//! changed between them as a person reads the data - a JSON object's keys in any order, a CSV
//! table's rows paired by their key - beside the diff window's line diff, which stays the core's
//! (`docs/plugins.md`, "Compare"). `a` is the old (left) file, `b` the new (right) one.

use serde::{Deserialize, Serialize};

use crate::{Failure, Source, SourceInfo};

/// The most changes one `compare` may answer; a longer list is refused as a whole. A plugin that
/// finds more lists this many and says `truncated`.
pub const MAX_CHANGES: usize = 10_000;

/// The most notices one `compare` may answer; the host keeps no more.
pub const MAX_NOTICES: usize = 100;

/// Whether the plugin compares now: a paid plugin without a valid licence says `unlicensed`,
/// and the diff window shows its text comparison alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareState {
    Ready,
    Unlicensed,
}

/// What became of one value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Only `b` has it.
    Added,
    /// Only `a` has it.
    Removed,
    /// Both have it, with other values.
    Changed,
    /// Both have it, equal, at another place: a reordered array element paired by its key.
    Moved,
}

impl ChangeKind {
    /// Every kind, in the contract's order.
    pub const ALL: [Self; 4] = [Self::Added, Self::Removed, Self::Changed, Self::Moved];

    /// The name the WIT enum uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
            Self::Moved => "moved",
        }
    }
}

/// One change between the two files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub kind: ChangeKind,
    /// Where: a JSON Pointer (`/items/3/name`), or a CSV row's key values.
    pub path: String,
    /// A CSV cell's column, by its header (or its number from 1 without one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    /// The value in `a`, as text; the host cuts a long one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    /// The value in `b`, as text; the host cuts a long one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    /// The line of `a` the value starts on, from 1: where the diff window jumps to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_line: Option<u32>,
    /// The line of `b` the value starts on, from 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
}

/// Something the person should know beside the changes - a key found twice, a row without its
/// key - as a code of the plugin's locales with its parameters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub code: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<(String, String)>,
}

/// The changes counted by kind; `truncated` when the plugin found more than it lists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompareSummary {
    pub added: u32,
    pub removed: u32,
    pub changed: u32,
    pub moved: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl CompareSummary {
    /// One more change of `kind`.
    pub fn count(&mut self, kind: ChangeKind) {
        let counter = self.of_mut(kind);
        *counter = counter.saturating_add(1);
    }

    /// How many changes of `kind`.
    #[must_use]
    pub const fn of(&self, kind: ChangeKind) -> u32 {
        match kind {
            ChangeKind::Added => self.added,
            ChangeKind::Removed => self.removed,
            ChangeKind::Changed => self.changed,
            ChangeKind::Moved => self.moved,
        }
    }

    fn of_mut(&mut self, kind: ChangeKind) -> &mut u32 {
        match kind {
            ChangeKind::Added => &mut self.added,
            ChangeKind::Removed => &mut self.removed,
            ChangeKind::Changed => &mut self.changed,
            ChangeKind::Moved => &mut self.moved,
        }
    }

    /// Every change counted.
    #[must_use]
    pub fn total(&self) -> u64 {
        ChangeKind::ALL
            .iter()
            .map(|kind| u64::from(self.of(*kind)))
            .sum()
    }
}

/// What one `compare` found.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub summary: CompareSummary,
    pub changes: Vec<Change>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<Notice>,
}

impl Comparison {
    /// No change at all: the two files hold the same data.
    #[must_use]
    pub fn is_equal(&self) -> bool {
        self.summary.total() == 0
    }
}

/// What a compare plugin may ask of its host: the `host`, `source` and `settings` interfaces of
/// its world.
pub trait CompareHost {
    /// Writes a line to rCommander's log; the host caps and masks it.
    fn log(&mut self, level: &str, message: &str);
    /// The time, Unix seconds: a licence's validity is checked against it.
    fn now_unix_seconds(&mut self) -> u64;
    /// The value of the manifest's setting `name`; `None` for a name it does not declare.
    fn setting(&mut self, name: &str) -> Option<String>;
    /// The source's name, size and time, never its folder.
    fn info(&mut self, src: Source) -> SourceInfo;
    /// At most 1 MiB of the source from `offset`.
    fn read_at(&mut self, src: Source, offset: u64, length: u32) -> Result<Vec<u8>, Failure>;
    /// Whether the person stopped the comparison.
    fn should_stop(&mut self) -> bool;
}

/// The logic of a compare plugin: `interface compare`. A store lives for one call.
pub trait Compare {
    /// Whether it compares now.
    fn status(&mut self, host: &mut dyn CompareHost) -> CompareState;

    /// The changes from `a` to `b`, at most [`MAX_CHANGES`] of them.
    fn compare(
        &mut self,
        host: &mut dyn CompareHost,
        a: Source,
        b: Source,
    ) -> Result<Comparison, Failure>;
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
