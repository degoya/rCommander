//! The coverage rows of the `inventory` group (`rc-engine`'s `registry/inventory.rs`), sorted by
//! id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("inventory.refresh", Coverage::Covered("inventory_refresh")),
    (
        "inventory.source_delete",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "inventory.source_save",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("inventory.sources", Coverage::Covered("inventory_sources")),
];
