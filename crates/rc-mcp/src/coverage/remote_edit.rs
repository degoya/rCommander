//! The coverage rows of the `remote_edit` group (`rc-engine`'s `registry/remote_edit.rs`), sorted
//! by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "remote_edit.close",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("remote_edit.list", Coverage::Covered("remote_edits_list")),
    (
        "remote_edit.skip",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "remote_edit.upload",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "remote_edit.window",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
