//! The coverage rows of the `drives` group (`rc-engine`'s `registry/drives.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("drives.list", Coverage::Covered("drives_list")),
    (
        "drives.map_network",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "drives.unmap_network",
        Coverage::Covered("drives_unmap_network"),
    ),
];
