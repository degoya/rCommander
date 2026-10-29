//! The coverage rows of the `archive_ops` group (`rc-engine`'s `registry/archive_ops.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("archive.extract", Coverage::Covered("archives_extract")),
    ("archive.pack", Coverage::Covered("archives_pack")),
    (
        "archive.password",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
