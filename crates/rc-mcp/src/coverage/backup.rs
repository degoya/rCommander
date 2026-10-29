//! The coverage rows of the `backup` group (`rc-engine`'s `registry/backup.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("db.backup", Coverage::Covered("db_backup")),
    (
        "db.backup_to",
        Coverage::Omitted("GUI only: human decision"),
    ),
    ("db.restore", Coverage::Omitted("GUI only: human decision")),
];
