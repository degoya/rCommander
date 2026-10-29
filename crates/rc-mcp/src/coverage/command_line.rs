//! The coverage rows of the `command_line` group (`rc-engine`'s `registry/command_line.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "cmdline.run",
        Coverage::Omitted(
            "starts a program in a console window for the person at the screen; agents run commands with terminal_open and terminal_send_input (exec, confirmed)",
        ),
    ),
    ("panel.filter", Coverage::Covered("panel_filter")),
];
