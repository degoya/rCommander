//! Why a comparison of two files or a synchronization of two folders cannot go on. Split off
//! [`EngineError`](super::EngineError) in RC-110-35; the codes are in `codes.rs`.

/// A `compare.*` or `sync.*` command refused; each variant keeps its own code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompareError {
    /// `compare.files` without paths found no two files: neither two selected files in the
    /// active panel nor a file under each panel's cursor.
    #[error("select two files, or put each panel's cursor on a file, to compare them")]
    CompareNeedsFiles,
    /// A compared file changed since it was read; the edit mode or `compare.apply_blocks` did
    /// not write over it (RC-100-94).
    #[error("{name} changed since it was compared; compare again")]
    CompareChanged { name: String },
    /// A compared file is not edited: binary, too large, or not written back as it was read.
    #[error("{name} cannot be changed here: {reason}")]
    CompareNotEditable { name: String, reason: &'static str },
    /// `compare.apply_blocks` names a block the comparison does not have.
    #[error("the comparison has no block {block}")]
    CompareBlockNotFound { block: usize },
    /// A synchronization would delete and the deletes were not confirmed; nothing was done.
    #[error("the synchronization deletes {count} entries; confirm the deletes to run it")]
    SyncDeletesUnconfirmed { count: usize },
    /// A synchronization row does not exist or cannot take the action.
    #[error("row {row} of the synchronization cannot take the action {action:?}")]
    SyncActionInvalid {
        row: usize,
        action: crate::sync::SyncAction,
    },
    /// `sync.apply` names a path the comparison has no row for.
    #[error("the synchronization has no row {path}")]
    SyncRowNotFound { path: String },
}
