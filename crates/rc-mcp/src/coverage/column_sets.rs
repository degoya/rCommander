//! The coverage rows of the `column_sets` group (`rc-engine`'s `registry/column_sets.rs`), sorted
//! by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "column_sets.create",
        Coverage::Covered("column_sets_create"),
    ),
    (
        "column_sets.delete",
        Coverage::Covered("column_sets_delete"),
    ),
    ("column_sets.get", Coverage::Covered("column_sets_get")),
    ("column_sets.list", Coverage::Covered("column_sets_list")),
    ("column_sets.move", Coverage::Covered("column_sets_move")),
    (
        "column_sets.rename",
        Coverage::Covered("column_sets_rename"),
    ),
    (
        "column_sets.update",
        Coverage::Covered("column_sets_update"),
    ),
];
