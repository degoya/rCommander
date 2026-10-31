//! The own colour themes in an export (RC-100-50): they go with the settings, which name them in
//! `appearance.theme_light` and `appearance.theme_dark`, so each keeps its id. The built-in themes
//! are code and never exported.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One own theme as an export holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedTheme {
    /// The theme's id, as the settings name it.
    pub id: String,
    pub name: String,
    pub dark: bool,
    /// `system` or `#rrggbb`.
    pub accent: String,
    /// `token -> "#rrggbb"`.
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}
