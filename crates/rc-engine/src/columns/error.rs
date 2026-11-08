//! Why a `column_sets.*` command or a column-set view was refused (RC-100-90), each with a stable
//! code; the window explains it under `error.<code>`.

use super::{ColumnSetId, MAX_SETS, SPEC_VERSION};
use crate::EngineError;

/// A refusal of a column set or its spec.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ColumnSetError {
    /// The spec is of another version than this build reads.
    #[error(
        "the column set has version {found}; this rCommander reads version {}",
        SPEC_VERSION
    )]
    Version { found: u64 },
    /// The spec is larger than [`super::MAX_SPEC_BYTES`].
    #[error("the column set has {bytes} bytes; at most {max} are kept")]
    TooLarge { bytes: usize, max: usize },
    /// The spec does not read or breaks a limit: no column, too many, a width out of bounds, an
    /// unknown field, a mask that does not fit.
    #[error("the column set does not fit: {reason}")]
    Invalid { reason: String },
    /// A name that is empty, too long or holds a control character.
    #[error("the name does not fit: {reason}")]
    Name { reason: String },
    /// Another column set has this name (case does not count).
    #[error("another column set is named {name:?}")]
    NameTaken { name: String },
    /// There are [`MAX_SETS`] column sets already.
    #[error("there are {} column sets already; delete one first", MAX_SETS)]
    TooMany,
    /// No column set has this id.
    #[error("there is no column set {id}")]
    Unknown { id: ColumnSetId },
}

impl ColumnSetError {
    /// Every code of these refusals.
    pub const ALL_CODES: &[&str] = &[
        "column_set_version",
        "column_set_too_large",
        "column_set_invalid",
        "column_set_name",
        "column_set_name_taken",
        "column_set_too_many",
        "column_set_unknown",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Version { .. } => "column_set_version",
            Self::TooLarge { .. } => "column_set_too_large",
            Self::Invalid { .. } => "column_set_invalid",
            Self::Name { .. } => "column_set_name",
            Self::NameTaken { .. } => "column_set_name_taken",
            Self::TooMany => "column_set_too_many",
            Self::Unknown { .. } => "column_set_unknown",
        }
    }
}

impl From<ColumnSetError> for EngineError {
    fn from(error: ColumnSetError) -> Self {
        Self::ColumnSet(error)
    }
}
