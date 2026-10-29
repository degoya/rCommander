//! The coverage rows of the `multi_rename` group (`rc-engine`'s `registry/multi_rename.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "multi_rename.apply",
        Coverage::Covered("multi_rename_apply"),
    ),
    (
        "multi_rename.open_dialog",
        Coverage::Omitted("purely visual; the preview is multi_rename_preview"),
    ),
    (
        "multi_rename.preset_delete",
        Coverage::Covered("multi_rename_preset_delete"),
    ),
    (
        "multi_rename.preset_list",
        Coverage::Covered("multi_rename_preset_list"),
    ),
    (
        "multi_rename.preset_rename",
        Coverage::Covered("multi_rename_preset_rename"),
    ),
    (
        "multi_rename.preset_save",
        Coverage::Covered("multi_rename_preset_save"),
    ),
    (
        "multi_rename.preview",
        Coverage::Covered("multi_rename_preview"),
    ),
    ("multi_rename.undo", Coverage::Covered("multi_rename_undo")),
];
