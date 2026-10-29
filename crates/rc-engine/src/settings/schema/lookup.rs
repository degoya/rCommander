//! A setting's row by its key, and whether only a person in the window may change it; split
//! from `schema.rs` to keep it under 500 lines (RC-020-12).

use super::{SETTINGS, SettingSpec};

/// The schema row of `key`.
#[must_use]
pub fn setting(key: &str) -> Option<&'static SettingSpec> {
    SETTINGS.iter().find(|spec| spec.key == key)
}

/// Whether only a person in the window may change `key`: every `mcp.*` and `vault.*` key (the
/// token and the policy among them, which are no schema rows), every `plugins.*` and `plugin.*`
/// key (they decide which foreign code runs, RC-100-10), every `update.*` key (they decide
/// which program runs next, RC-100-05), and every row marked protected. An agent must never
/// loosen its own permissions.
#[must_use]
pub fn is_protected(key: &str) -> bool {
    ["mcp.", "vault.", "plugins.", "plugin.", "update."]
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || setting(key).is_some_and(|s| s.protected)
}
