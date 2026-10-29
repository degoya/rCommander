//! The errors of the rename engine, each with a stable code.

use std::fmt;

use serde::Serialize;

/// Which of the two masks a [`PatternError`] is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskField {
    /// The mask for the name without the extension.
    Name,
    /// The mask for the extension.
    Extension,
}

impl fmt::Display for MaskField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "name",
            Self::Extension => "extension",
        })
    }
}

/// A mask that does not parse: where, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("position {position}: {kind}")]
pub struct PatternError {
    /// The character offset (0-based, in Unicode scalar values) of the `[` that opens the
    /// placeholder, or of the stray `]`.
    pub position: usize,
    /// What is wrong there.
    pub kind: PatternErrorKind,
}

/// The kinds of [`PatternError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PatternErrorKind {
    /// A `[` without its `]`, or a `[` inside a placeholder.
    #[error("the placeholder is not closed")]
    Unclosed,
    /// A `]` outside a placeholder; a literal one is written `[]]`.
    #[error("a ']' outside a placeholder")]
    StrayBracket,
    /// `[]`.
    #[error("an empty placeholder")]
    Empty,
    /// A placeholder rCommander does not know (Total Commander's `[=plugin.field]`, `[d]`,
    /// `[t]`, `[S]`, `[R]` among them).
    #[error("unknown placeholder [{text}]")]
    Unknown { text: String },
    /// A known placeholder with arguments it does not take, such as `[N0]` or `[Caa+1]`.
    #[error("invalid arguments in [{text}]")]
    Arguments { text: String },
}

/// What stops a rename plan or its schedule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RenameError {
    /// A mask, search text or replacement is longer than the rule allows.
    #[error("the {field} is longer than {max} characters")]
    TooLong { field: &'static str, max: usize },
    /// One of the masks does not parse.
    #[error("{field} mask, {error}")]
    Mask {
        field: MaskField,
        error: PatternError,
    },
    /// The search expression is not a valid regular expression.
    #[error("invalid regular expression: {message}")]
    Regex { message: String },
    /// A plan with conflicts cannot be carried out.
    #[error("the plan has {count} conflicts")]
    Conflicts { count: usize },
    /// The renames could not be put in an order; a defect, never expected.
    #[error("the renames cannot be put in a safe order")]
    Unschedulable,
}

impl RenameError {
    /// Every code [`RenameError::code`] returns.
    pub const ALL_CODES: &[&str] = &[
        "rename_input_too_long",
        "rename_mask_unclosed",
        "rename_mask_stray_bracket",
        "rename_mask_empty",
        "rename_mask_unknown",
        "rename_mask_arguments",
        "rename_regex_invalid",
        "rename_plan_conflicts",
        "rename_unschedulable",
    ];

    /// The stable code: the GUI looks its message up under `error.<code>`, the MCP server
    /// returns it to agents.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLong { .. } => "rename_input_too_long",
            Self::Mask { error, .. } => match error.kind {
                PatternErrorKind::Unclosed => "rename_mask_unclosed",
                PatternErrorKind::StrayBracket => "rename_mask_stray_bracket",
                PatternErrorKind::Empty => "rename_mask_empty",
                PatternErrorKind::Unknown { .. } => "rename_mask_unknown",
                PatternErrorKind::Arguments { .. } => "rename_mask_arguments",
            },
            Self::Regex { .. } => "rename_regex_invalid",
            Self::Conflicts { .. } => "rename_plan_conflicts",
            Self::Unschedulable => "rename_unschedulable",
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
