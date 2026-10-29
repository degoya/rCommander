//! The coverage rows of the `tabs` group (`rc-engine`'s `registry/tabs.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("panel.swap", Coverage::Covered("panel_swap")),
    ("panel.transfer", Coverage::Covered("panel_transfer")),
    ("tabs.close", Coverage::Covered("tab_close")),
    ("tabs.copy_to_other", Coverage::Covered("tab_copy_to_other")),
    ("tabs.list", Coverage::Covered("tab_list")),
    ("tabs.lock", Coverage::Covered("tab_lock")),
    ("tabs.move", Coverage::Covered("tab_move")),
    (
        "tabs.next",
        Coverage::Omitted("Ctrl+Tab, a keyboard step; tab_select names the tab"),
    ),
    ("tabs.open", Coverage::Covered("tab_open")),
    ("tabs.select", Coverage::Covered("tab_select")),
];
