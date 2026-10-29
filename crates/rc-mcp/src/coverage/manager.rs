//! The coverage rows of the `manager` group (`rc-engine`'s `registry/manager.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "connections.manager",
        Coverage::Omitted("opens a dialog in the window; the connections_ tools do what it offers"),
    ),
    (
        "connections.set_favourite",
        Coverage::Covered("connections_set_favourite"),
    ),
];
