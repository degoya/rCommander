//! The coverage rows of the `panel_lists` group (`rc-engine`'s `registry/panel_lists.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("filters.delete", Coverage::Covered("filters_delete")),
    ("filters.list", Coverage::Covered("filters_list")),
    ("filters.rename", Coverage::Covered("filters_rename")),
    ("filters.save", Coverage::Covered("filters_save")),
    ("panel.file_filter", Coverage::Covered("panel_file_filter")),
    ("panel.load_list", Coverage::Covered("panel_load_list")),
    (
        "panel.save_selection",
        Coverage::Covered("panel_save_selection"),
    ),
    (
        "panel.selection_from_file",
        Coverage::Covered("panel_selection_from_file"),
    ),
    (
        "panel.selection_from_text",
        Coverage::Covered("panel_selection_from_text"),
    ),
    (
        "panel.selection_to_file",
        Coverage::Covered("panel_selection_to_file"),
    ),
    (
        "panel.toggle_ignore_list",
        Coverage::Covered("panel_toggle_ignore_list"),
    ),
];
