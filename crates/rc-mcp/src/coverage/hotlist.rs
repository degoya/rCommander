//! The coverage rows of the `hotlist` group (`rc-engine`'s `registry/hotlist.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("hotlist.add", Coverage::Covered("hotlist_add")),
    (
        "hotlist.add_folder",
        Coverage::Covered("hotlist_add_folder"),
    ),
    ("hotlist.delete", Coverage::Covered("hotlist_delete")),
    ("hotlist.list", Coverage::Covered("hotlist_list")),
    ("hotlist.move", Coverage::Covered("hotlist_move")),
    ("hotlist.open", Coverage::Covered("hotlist_open")),
    ("hotlist.update", Coverage::Covered("hotlist_update")),
];
