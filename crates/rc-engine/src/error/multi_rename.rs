//! Why a multi-rename, its undo or one of its presets cannot go on; a rule's own refusal is
//! [`rc_rename::RenameError`]. Split off [`EngineError`](super::EngineError) in RC-110-35 (the
//! variants were `Rename*` there); the codes are in `codes.rs`.

use rc_rename::UndoMismatch;

/// A multi-rename refused; each variant keeps its own code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MultiRenameError {
    /// A multi-rename or its undo is still queued or running.
    #[error("a multi-rename is still running; wait until it has finished")]
    Busy,
    /// The rule gives every entry the name it has.
    #[error("the rule changes no name; nothing was renamed")]
    NoChanges,
    /// The undo log holds no run with a step carried out.
    #[error("there is no multi-rename to undo")]
    NothingToUndo,
    /// The latest run is no longer whole in the undo log (cut by its retention limit) or cannot
    /// be read; it is not undone in part.
    #[error("the latest multi-rename is no longer complete in the undo log and cannot be undone")]
    UndoIncomplete,
    /// Entries of the latest run were renamed, removed or taken since; nothing was undone.
    #[error(
        "{} entries no longer fit the undo, nothing was renamed: {}",
        .mismatches.len(),
        super::convert::describe_mismatches(.mismatches)
    )]
    UndoMismatch { mismatches: Vec<UndoMismatch> },
    /// A multi-rename preset name that cannot be used, or one preset too many.
    #[error("the preset is refused: {reason}")]
    PresetInvalid { reason: String },
    /// No multi-rename preset has this name.
    #[error("there is no preset named {name}")]
    PresetNotFound { name: String },
}
