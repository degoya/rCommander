//! The coverage rows of the `file` group (`rc-engine`'s `registry/file.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("file.combine", Coverage::Covered("files_combine")),
    ("file.open", Coverage::Covered("file_open")),
    ("file.split", Coverage::Covered("files_split")),
];
