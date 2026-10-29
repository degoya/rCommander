//! The coverage rows of the `secrets` group (`rc-engine`'s `registry/secrets.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "secrets.import_key",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "secrets.source_map",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "secrets.source_search",
        Coverage::Covered("credential_sources_search"),
    ),
    (
        "secrets.source_unmap",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "secrets.sources",
        Coverage::Covered("credential_sources_list"),
    ),
];
