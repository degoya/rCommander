//! The coverage rows of the `history` group (`rc-engine`'s `registry/history.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("history.back", Coverage::Covered("history_back")),
    ("history.forward", Coverage::Covered("history_forward")),
    ("history.jump", Coverage::Covered("history_jump")),
    ("history.list", Coverage::Covered("history_list")),
];
