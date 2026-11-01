//! One file's digest as `checksum.compute` answers it (moved out of `state.rs` to keep it under
//! 500 lines).

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

/// One file's digest, or why it has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDigest {
    pub path: VfsPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
    /// The error code when the file could not be hashed (`not_found`, `is_a_directory`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
