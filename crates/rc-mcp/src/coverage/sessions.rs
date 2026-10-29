//! The coverage rows of the `sessions` group (`rc-engine`'s `registry/sessions.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    (
        "connections.connect",
        Coverage::Covered("connections_connect"),
    ),
    (
        "connections.disconnect",
        Coverage::Covered("connections_disconnect"),
    ),
    (
        "connections.session_prefs",
        Coverage::Covered("connections_session_prefs"),
    ),
    (
        "connections.set_session_prefs",
        Coverage::Covered("connections_set_session_prefs"),
    ),
    ("connections.test", Coverage::Covered("connections_test")),
];
