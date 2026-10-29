//! The coverage rows of the `checksum` group (`rc-engine`'s `registry/checksum.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("checksum.compute", Coverage::Covered("files_checksum")),
    (
        "checksum.create",
        Coverage::Covered("files_checksum_create"),
    ),
    (
        "checksum.dialog",
        Coverage::Omitted("opens a dialog in the window"),
    ),
    (
        "checksum.queue",
        Coverage::Omitted("the window's queued form of the four checksum tools"),
    ),
    (
        "checksum.verify",
        Coverage::Covered("files_checksum_verify"),
    ),
];
