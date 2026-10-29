//! The coverage rows of the `compare` group (`rc-engine`'s `registry/compare.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("compare.dirs", Coverage::Covered("dirs_compare")),
    ("compare.files", Coverage::Covered("files_compare")),
    (
        "compare.open_diff",
        Coverage::Omitted("opens the diff window; files_compare compares"),
    ),
    (
        "compare.select_differences",
        Coverage::Covered("panel_select_differences"),
    ),
    ("sync.apply", Coverage::Covered("dirs_sync_apply")),
    (
        "sync.open_dialog",
        Coverage::Omitted("opens a dialog in the window; see dirs_compare"),
    ),
];
