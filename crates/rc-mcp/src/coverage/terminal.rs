//! The coverage rows of the `terminal` group (`rc-engine`'s `registry/terminal.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("terminal.close", Coverage::Covered("terminal_close")),
    ("terminal.list", Coverage::Covered("terminal_list")),
    (
        "terminal.maximize",
        Coverage::Omitted(
            "purely visual, a toggle of the window's layout; terminal_read_screen reads the terminal at any width",
        ),
    ),
    ("terminal.open", Coverage::Covered("terminal_open")),
    // It reads the person's clipboard and uploads it to the terminal's server: their key only.
    (
        "terminal.paste_image",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "terminal.read_screen",
        Coverage::Covered("terminal_read_screen"),
    ),
    (
        "terminal.send_input",
        Coverage::Covered("terminal_send_input"),
    ),
    ("terminal.show", Coverage::Covered("terminal_show")),
];
