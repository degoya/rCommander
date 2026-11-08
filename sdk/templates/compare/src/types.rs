//! The compare world's shapes this plugin uses, mirrored from `wit/rcommander.wit`, so the logic
//! in `lib.rs` is plain Rust that `cargo test` runs without a WebAssembly toolchain.
//! `src/guest.rs` maps each to its WIT twin and nothing more.

/// `change-kind`: what became of one value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
    Moved,
}

/// `change`: one change from the old file (`a`) to the new one (`b`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    /// Where: here the key.
    pub path: String,
    pub column: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    /// The lines the value is on, from 1: where rCommander's text view jumps to.
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

/// `notice`: something the person should know, as a code of `locales/en.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub code: String,
    pub params: Vec<(String, String)>,
}

/// `comparison`: the changes, counted by kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    pub changes: Vec<Change>,
    pub notices: Vec<Notice>,
    /// More changes than [`crate::MAX_CHANGES`] were found; the list holds the first.
    pub truncated: bool,
}

impl Comparison {
    /// How many changes of `kind` the list holds.
    #[must_use]
    pub fn count(&self, kind: ChangeKind) -> u32 {
        let count = self
            .changes
            .iter()
            .filter(|change| change.kind == kind)
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}

/// `failure-kind`, the cases this plugin answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    BadData,
    Io,
}

/// `failure`: a kind and an optional code from `locales/en.json`, which starts with the plugin's
/// id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub code: Option<String>,
}

impl Failure {
    /// A failure of `kind` without a code of its own.
    #[must_use]
    pub fn of(kind: FailureKind) -> Self {
        Self { kind, code: None }
    }
}
