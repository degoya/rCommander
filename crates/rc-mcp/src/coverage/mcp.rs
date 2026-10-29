//! The coverage rows of the `mcp` group (`rc-engine`'s `registry/mcp.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("mcp.answer", Coverage::Omitted("GUI only: human decision")),
    ("mcp.revoke", Coverage::Omitted("GUI only: human decision")),
];
