//! The coverage rows of the `diagnostics` group (`rc-engine`'s `registry/diagnostics.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "diagnostics.clear_error_log",
        Coverage::Covered("diagnostics_clear_error_log"),
    ),
    (
        "diagnostics.error_log",
        Coverage::Covered("diagnostics_error_log"),
    ),
    (
        "diagnostics.get_retention",
        Coverage::Covered("diagnostics_get_retention"),
    ),
    (
        "diagnostics.set_retention",
        Coverage::Covered("diagnostics_set_retention"),
    ),
];
