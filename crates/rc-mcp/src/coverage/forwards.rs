//! The coverage rows of the `forwards` group (`rc-engine`'s `registry/forwards.rs`), sorted by
//! id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("forwards.delete", Coverage::Covered("forwards_delete")),
    ("forwards.list", Coverage::Covered("forwards_list")),
    // A listening port is a person's decision (RC-110-03).
    (
        "forwards.save",
        Coverage::Omitted("GUI only: human decision"),
    ),
    // A listening port is a person's decision (RC-110-03).
    (
        "forwards.start",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("forwards.stop", Coverage::Covered("forwards_stop")),
    ("forwards.stop_all", Coverage::Covered("forwards_stop_all")),
];
