//! The coverage rows of the `ssh_exec` group (`rc-engine`'s `registry/ssh_exec.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[("ssh.exec", Coverage::Covered("ssh_exec"))];
