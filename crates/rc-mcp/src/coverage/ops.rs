//! The coverage rows of the `ops` group (`rc-engine`'s `registry/ops.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "ops.answer",
        Coverage::Omitted(
            "MCP jobs carry their answers as presets (on_conflict, on_error) and never ask",
        ),
    ),
    ("ops.cancel", Coverage::Covered("ops_cancel")),
    ("ops.copy", Coverage::Covered("ops_copy")),
    ("ops.delete", Coverage::Covered("ops_delete")),
    ("ops.list", Coverage::Covered("ops_list")),
    ("ops.mkdir", Coverage::Covered("ops_mkdir")),
    ("ops.move", Coverage::Covered("ops_move")),
    ("ops.pause", Coverage::Covered("ops_pause")),
    ("ops.resume", Coverage::Covered("ops_resume")),
];
