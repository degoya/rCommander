//! The coverage rows of the `workspaces` group (`rc-engine`'s `registry/workspaces.rs`), sorted by
//! id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("workspaces.apply", Coverage::Covered("workspaces_apply")),
    (
        "workspaces.capture",
        Coverage::Covered("workspaces_capture"),
    ),
    ("workspaces.create", Coverage::Covered("workspaces_create")),
    ("workspaces.delete", Coverage::Covered("workspaces_delete")),
    ("workspaces.get", Coverage::Covered("workspaces_get")),
    ("workspaces.list", Coverage::Covered("workspaces_list")),
    (
        "workspaces.manager",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("workspaces.move", Coverage::Covered("workspaces_move")),
    ("workspaces.rename", Coverage::Covered("workspaces_rename")),
    ("workspaces.update", Coverage::Covered("workspaces_update")),
];
