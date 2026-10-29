//! The coverage rows of the `panel` group (`rc-engine`'s `registry/panel.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "panel.cancel_io",
        Coverage::Omitted(
            "Escape in the window for a drive that does not answer; an MCP call ends at its time limit or when the client cancels it",
        ),
    ),
    (
        "panel.change_drive",
        Coverage::Covered("panel_change_drive"),
    ),
    (
        "panel.context_menu",
        Coverage::Omitted("GUI only: shell menu"),
    ),
    ("panel.deselect_all", Coverage::Covered("panel_select")),
    (
        "panel.deselect_by_mask",
        Coverage::Covered("panel_deselect_by_mask"),
    ),
    (
        "panel.drive_menu",
        Coverage::Omitted(
            "opens a menu in the window; drives_list and panel_change_drive do what it offers",
        ),
    ),
    ("panel.get_state", Coverage::Covered("get_panel_state")),
    ("panel.invert_selection", Coverage::Covered("panel_select")),
    ("panel.list_dir", Coverage::Covered("list_dir")),
    (
        "panel.move_cursor",
        Coverage::Omitted(
            "a keyboard step; set_cursor reaches any entry by index or name, and a page depends on the window",
        ),
    ),
    ("panel.navigate", Coverage::Covered("navigate")),
    (
        "panel.open_entry",
        Coverage::Omitted("Ctrl+PageDown; file_open and navigate open a directory by its name"),
    ),
    (
        "panel.open_terminal_here",
        Coverage::Omitted(
            "terminal_open takes the panel and a cwd: a shell in any panel's folder, shown by any panel",
        ),
    ),
    (
        "panel.refresh",
        Coverage::Omitted("navigate to the panel's own path reads it again"),
    ),
    ("panel.restore_selection", Coverage::Covered("panel_select")),
    ("panel.select_all", Coverage::Covered("panel_select")),
    (
        "panel.select_by_mask",
        Coverage::Covered("panel_select_by_mask"),
    ),
    ("panel.select_extension", Coverage::Covered("panel_select")),
    (
        "panel.select_range",
        Coverage::Omitted(
            "a keyboard range from the cursor; panel_select takes masks and toggle_selection names",
        ),
    ),
    ("panel.set_cursor", Coverage::Covered("set_cursor")),
    ("panel.set_view", Coverage::Covered("panel_set_view")),
    ("panel.show_files", Coverage::Covered("show_files")),
    ("panel.sort", Coverage::Covered("panel_sort")),
    (
        "panel.switch_active",
        Coverage::Covered("switch_active_panel"),
    ),
    (
        "panel.toggle_cursor_selection",
        Coverage::Omitted("the Insert key; toggle_selection names the entries"),
    ),
    (
        "panel.toggle_hidden",
        Coverage::Covered("panel_show_hidden"),
    ),
    (
        "panel.toggle_selection",
        Coverage::Covered("toggle_selection"),
    ),
    (
        "panel.toggle_terminal",
        Coverage::Omitted(
            "a toggle depends on what the window shows; terminal_open, terminal_show and show_files name the result",
        ),
    ),
];
