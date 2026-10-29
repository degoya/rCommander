//! The coverage rows of the `login_scripts` group (`rc-engine`'s `registry/login_scripts.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "login_scripts.assign",
        Coverage::Covered("login_scripts_assign"),
    ),
    (
        "login_scripts.clear_secret",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "login_scripts.create",
        Coverage::Covered("login_scripts_create"),
    ),
    (
        "login_scripts.delete",
        Coverage::Covered("login_scripts_delete"),
    ),
    ("login_scripts.get", Coverage::Covered("login_scripts_get")),
    (
        "login_scripts.list",
        Coverage::Covered("login_scripts_list"),
    ),
    (
        "login_scripts.set_secret",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "login_scripts.update",
        Coverage::Covered("login_scripts_update"),
    ),
];
