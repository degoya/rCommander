//! The coverage rows of the `archive` group (`rc-engine`'s `registry/archive.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("archive.list", Coverage::Covered("archives_list")),
    ("archive.test", Coverage::Covered("archives_test")),
];
