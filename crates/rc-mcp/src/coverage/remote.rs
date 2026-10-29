//! The coverage rows of the `remote` group (`rc-engine`'s `registry/remote.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "connections.quick_connect",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("files.stat", Coverage::Covered("files_stat")),
];
