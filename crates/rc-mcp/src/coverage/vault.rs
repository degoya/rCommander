//! The coverage rows of the `vault` group (`rc-engine`'s `registry/vault.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "vault.change_password",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "vault.create",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("vault.lock", Coverage::Covered("vault_lock")),
    (
        "vault.remove",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("vault.status", Coverage::Covered("vault_status")),
    (
        "vault.unlock",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
