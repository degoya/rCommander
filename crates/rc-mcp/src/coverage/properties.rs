//! The coverage rows of the `properties` group (`rc-engine`'s `registry/properties.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "clipboard.copy_names",
        Coverage::Omitted(
            "the desktop clipboard, of no use to an agent; list_dir names the entries",
        ),
    ),
    ("file.create_link", Coverage::Covered("files_create_link")),
    ("file.dir_size", Coverage::Covered("files_dir_size")),
    ("file.properties", Coverage::Covered("files_properties")),
    (
        "file.set_attributes",
        Coverage::Covered("files_set_attributes"),
    ),
    ("file.set_comment", Coverage::Covered("files_set_comment")),
    (
        "panel.cancel_dir_sizes",
        Coverage::Omitted(
            "stops the window's size column count; an MCP call stops when the client cancels it",
        ),
    ),
    (
        "panel.count_dir_content",
        Coverage::Omitted("fills the window's size column; files_dir_size answers the sizes"),
    ),
    (
        "panel.select_and_size",
        Coverage::Omitted("the Space key: toggle_selection and files_dir_size do its two halves"),
    ),
];
