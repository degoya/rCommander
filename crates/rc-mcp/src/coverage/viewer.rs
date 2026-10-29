//! The coverage rows of the `viewer` group (`rc-engine`'s `registry/viewer.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("files.read_hex", Coverage::Covered("files_read_hex")),
    ("files.read_text", Coverage::Covered("files_read_text")),
    (
        "panel.toggle_quick_view",
        Coverage::Omitted("purely visual; files_read_text and files_read_hex read what it shows"),
    ),
    (
        "view.open",
        Coverage::Omitted("purely visual; files_read_text and files_read_hex read what it shows"),
    ),
];
