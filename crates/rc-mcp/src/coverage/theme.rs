//! The coverage rows of the `theme` group (`rc-engine`'s `registry/theme.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "theme.delete",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("theme.list", Coverage::Covered("theme_list")),
    ("theme.save", Coverage::Omitted("GUI only: human decision")),
    ("theme.select", Coverage::Covered("theme_select")),
];
