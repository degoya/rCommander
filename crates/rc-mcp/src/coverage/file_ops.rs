//! The coverage rows of the `file_ops` group (`rc-engine`'s `registry/file_ops.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "file.open_external",
        Coverage::Omitted(
            "starts the viewer or editor a person configured, for that person at the window; file_open hands a file to the system",
        ),
    ),
    (
        "ops.copy_to",
        Coverage::Omitted(
            "the F5 dialog's typed target; agents copy into a directory with ops_copy",
        ),
    ),
    (
        "ops.dialog",
        Coverage::Omitted("GUI dialog; the operation itself is an ops_* tool"),
    ),
    (
        "ops.move_to",
        Coverage::Omitted(
            "the F6 dialog's typed target; agents move into a directory with ops_move",
        ),
    ),
];
