//! The coverage rows of the `import` group (`rc-engine`'s `registry/import.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "config.export",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "config.import",
        Coverage::Omitted("GUI only: human decision"),
    ),
    (
        "connections.import_ssh_config",
        Coverage::Covered("connections_import_ssh_config"),
    ),
    (
        "import.remote_desktop_manager",
        Coverage::Covered("import_remote_desktop_manager"),
    ),
    (
        "import.total_commander",
        Coverage::Covered("import_total_commander"),
    ),
    (
        "import.total_commander_passwords",
        Coverage::Omitted("GUI only: human decision"),
    ),
];
