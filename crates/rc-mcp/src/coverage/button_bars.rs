//! The coverage rows of the `button_bars` group (`rc-engine`'s `registry/button_bars.rs`), sorted
//! by id. Which program a button runs is a person's choice, as an own command's is: saving,
//! renaming, deleting and importing bars is the window's.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "button_bar.delete",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "button_bar.focus",
        Coverage::Omitted("moves the window's keyboard focus; nothing an agent reads"),
    ),
    ("button_bar.get", Coverage::Covered("button_bars_get")),
    (
        "button_bar.import",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "button_bar.import_preview",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("button_bar.list", Coverage::Covered("button_bars_list")),
    (
        "button_bar.manager",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "button_bar.rename",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("button_bar.run", Coverage::Covered("button_bars_run")),
    (
        "button_bar.save",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "view.toggle_bar",
        Coverage::Omitted("agents switch a bar with settings_set"),
    ),
];
