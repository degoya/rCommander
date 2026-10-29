//! The coverage rows of the `plugins` group (`rc-engine`'s `registry/plugins.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "plugins.disable",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "plugins.enable",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "plugins.install",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("plugins.list", Coverage::Covered("plugins_list")),
    (
        "plugins.preview",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "plugins.remove",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "plugins.resume",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "plugins.rollback",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
