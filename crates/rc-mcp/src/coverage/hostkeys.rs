//! The coverage rows of the `hostkeys` group (`rc-engine`'s `registry/hostkeys.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "hostkeys.answer",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "hostkeys.import_known_hosts",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("hostkeys.list", Coverage::Covered("hostkeys_list")),
    (
        "hostkeys.remove",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "hostkeys.replace",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
