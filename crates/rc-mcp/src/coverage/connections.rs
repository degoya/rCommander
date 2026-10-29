//! The coverage rows of the `connections` group (`rc-engine`'s `registry/connections.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "connections.create",
        Coverage::Covered("connections_create"),
    ),
    (
        "connections.delete",
        Coverage::Covered("connections_delete"),
    ),
    (
        "connections.duplicate",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("connections.get", Coverage::Covered("connections_get")),
    ("connections.list", Coverage::Covered("connections_list")),
    ("connections.move", Coverage::Covered("connections_move")),
    ("connections.save", Coverage::Covered("connections_save")),
    (
        "connections.update",
        Coverage::Covered("connections_update"),
    ),
    (
        "credentials.delete",
        Coverage::Covered("credentials_delete"),
    ),
    ("credentials.move", Coverage::Covered("credentials_move")),
    ("credentials.save", Coverage::Covered("credentials_save")),
    (
        "folders.create",
        Coverage::Covered("connections_folder_create"),
    ),
    (
        "folders.delete",
        Coverage::Covered("connections_folder_delete"),
    ),
    ("folders.move", Coverage::Covered("connections_folder_move")),
    (
        "folders.rename",
        Coverage::Covered("connections_folder_rename"),
    ),
];
