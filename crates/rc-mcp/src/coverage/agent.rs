//! The coverage rows of the `agent` group (`rc-engine`'s `registry/agent.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[(
    "agent.list_identities",
    Coverage::Covered("ssh_agent_identities"),
)];
