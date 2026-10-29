//! The coverage rows of the `update` group (`rc-engine`'s `registry/update.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("update.check", Coverage::Covered("update_check")),
    (
        "update.download",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "update.install",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("update.status", Coverage::Covered("update_status")),
];
