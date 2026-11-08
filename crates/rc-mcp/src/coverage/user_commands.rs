//! The coverage rows of the `user_commands` group (`rc-engine`'s `registry/user_commands.rs`),
//! sorted by id. Which program an own command or a Start menu entry runs is a person's choice, as
//! the viewer program is: saving, deleting and moving them is the window's.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "start_menu.delete",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("start_menu.list", Coverage::Covered("start_menu_list")),
    (
        "start_menu.manager",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "start_menu.move",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("start_menu.run", Coverage::Covered("start_menu_run")),
    (
        "start_menu.save",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "usercmd.delete",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("usercmd.list", Coverage::Covered("user_commands_list")),
    (
        "usercmd.manager",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("usercmd.run", Coverage::Covered("user_commands_run")),
    (
        "usercmd.save",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
