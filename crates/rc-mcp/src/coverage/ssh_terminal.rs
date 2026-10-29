//! The coverage rows of the `ssh_terminal` group (`rc-engine`'s `registry/ssh_terminal.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] =
    &[("terminal.open_ssh", Coverage::Covered("terminal_open"))];
