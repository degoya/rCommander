//! The coverage rows of the `wsl` group (`rc-engine`'s `registry/wsl.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("wsl.list_distros", Coverage::Covered("wsl_list_distros")),
    ("wsl.refresh", Coverage::Covered("wsl_refresh")),
];
