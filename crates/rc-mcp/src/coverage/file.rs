//! The coverage rows of the `file` group (`rc-engine`'s `registry/file.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "file.associations",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("file.combine", Coverage::Covered("files_combine")),
    ("file.open", Coverage::Covered("file_open")),
    (
        "file.run_association",
        Coverage::Omitted(
            "starts a program a person associated, from that person's context menu; file_open runs a file's open command as Enter does",
        ),
    ),
    ("file.split", Coverage::Covered("files_split")),
];
