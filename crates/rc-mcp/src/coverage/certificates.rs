//! The coverage rows of the `certificates` group (`rc-engine`'s `registry/certificates.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "certificates.answer",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("certificates.list", Coverage::Covered("certificates_list")),
    (
        "certificates.remove",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
