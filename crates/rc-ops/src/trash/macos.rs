//! The Trash on macOS, through the `trash` crate (the decision is in `trash.rs`).
//!
//! `NSFileManager`'s `trashItemAtURL` rather than the crate's default, a Finder AppleScript: it
//! starts no `osascript`, asks for no automation permission and makes no sound. Finder's "Put
//! Back" may then be missing for the entry; dragging it out of the Trash restores it. A volume
//! without a Trash refuses, which comes back as `trash_failed` - never a permanent delete.

use std::path::Path;

use trash::{
    TrashContext,
    macos::{DeleteMethod, TrashContextExtMacos as _},
};

use crate::OpsError;

/// Moves `path` into the Trash; a link goes there itself, never what it points to - so the path
/// is not canonicalised here (the crate resolves its parent only).
pub(super) fn trash(path: &Path) -> Result<(), OpsError> {
    std::fs::symlink_metadata(path).map_err(|error| OpsError::from_io(error, path))?;
    let mut context = TrashContext::default();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context.delete(path).map_err(|error| OpsError::TrashFailed {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}
