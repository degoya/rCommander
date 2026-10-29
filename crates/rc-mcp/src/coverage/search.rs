//! The coverage rows of the `search` group (`rc-engine`'s `registry/search.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "files.find_dialog",
        Coverage::Omitted(
            "opens a dialog in the window; files_search runs the search and panel_feed_paths shows its hits in a panel",
        ),
    ),
    ("files.search", Coverage::Covered("files_search")),
    ("panel.branch_view", Coverage::Covered("panel_branch_view")),
    ("panel.feed_paths", Coverage::Covered("panel_feed_paths")),
];
