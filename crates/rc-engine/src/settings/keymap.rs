//! The keymap in force (RC-100-21): the standard with the deviations of the protected setting
//! [`KEYMAP_KEY`] - `keymap.list` reads it, `keymap.set` stores the deviations. `keymap.set` is
//! `GuiOnly` and the row protected, so `settings.set` refuses it too: a moved key changes what
//! one key press of a person runs, and only the person moves it. An import of the setting
//! (`config.import`) shows it as a protected row, unticked.

use serde_json::Value;

use super::{KEYMAP_KEY, stored_setting};
use crate::{Engine, EngineError, Keymap, Outcome, Override, Overrides};

impl Engine {
    /// The keymap in force: the standard with the stored deviations; the standard alone without
    /// a database. A deviation that no longer fits is left out with a warning.
    pub async fn keymap(&self) -> Keymap {
        self.keymap_and_stale().await.0
    }

    async fn keymap_and_stale(&self) -> (Keymap, Vec<Override>) {
        let text = match self.db() {
            Ok(db) => stored_setting(db, KEYMAP_KEY).await,
            Err(_) => None,
        };
        // The stored value was checked against its row: a text that does not read is the
        // default already.
        let overrides = text
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|text| Overrides::parse(text).ok())
            .unwrap_or_default();
        let (keymap, stale) = Keymap::with_overrides(&overrides);
        if !stale.is_empty() {
            tracing::warn!(
                count = stale.len(),
                "key overrides name rows the keymap no longer has; they are left out"
            );
        }
        (keymap, stale)
    }

    /// `keymap.list`: every row in force.
    pub(crate) async fn list_keymap(&self) -> Result<Outcome, EngineError> {
        let (keymap, stale) = self.keymap_and_stale().await;
        Ok(Outcome::Keymap {
            rows: keymap.rows(),
            stale: stale.iter().map(ToString::to_string).collect(),
        })
    }

    /// `keymap.set`: stores the deviations `overrides` (checked as the setting's row checks
    /// them), announces the setting and answers the rows in force.
    pub(crate) async fn set_keymap(&self, overrides: &str) -> Result<Outcome, EngineError> {
        self.set_protected_setting(KEYMAP_KEY, &Value::from(overrides))
            .await?;
        self.list_keymap().await
    }
}

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;
