//! The coverage rows of the `settings` group (`rc-engine`'s `registry/settings.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("keymap.list", Coverage::Covered("keymap_list")),
    ("keymap.set", Coverage::Omitted("GUI only: human decision")),
    ("settings.get", Coverage::Covered("settings_get")),
    ("settings.set", Coverage::Covered("settings_set")),
];
