//! The coverage rows of the `app` group (`rc-engine`'s `registry/app.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("app.about", Coverage::Covered("app_about")),
    ("app.quit", Coverage::Omitted("GUI only: human decision")),
];
