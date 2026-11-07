//! Why an inventory command failed; each with a stable code.

use rc_db::inventory::InventoryError;

/// An inventory source's failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InventorySourceError {
    /// The source refused or the database failed; keeps the code of its [`InventoryError`].
    #[error(transparent)]
    Store(#[from] InventoryError),
    /// No installed, enabled inventory plugin has the source's id (removed, switched off,
    /// plugins off).
    #[error("the inventory plugin of {name} is not installed or is switched off")]
    PluginMissing { name: String },
    /// The plugin failed or answered against the rules; `code` is the host's
    /// (`plugin_inventory_too_large`, `plugin_http_denied`, ...) or `plugin_failed`.
    #[error("the inventory {name} failed ({code})")]
    Failed { name: String, code: &'static str },
}

impl InventorySourceError {
    /// Every code of this enum's own variants; [`Self::Store`] carries one of
    /// [`InventoryError::ALL_CODES`], [`Self::Failed`] one of [`crate::plugins::ALL_CODES`].
    pub const ALL_CODES: &[&str] = &["inventory_plugin_missing"];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Store(error) => error.code(),
            Self::PluginMissing { .. } => "inventory_plugin_missing",
            Self::Failed { code, .. } => code,
        }
    }
}

impl From<InventorySourceError> for crate::EngineError {
    fn from(error: InventorySourceError) -> Self {
        Self::Inventory(error)
    }
}

impl From<InventoryError> for crate::EngineError {
    fn from(error: InventoryError) -> Self {
        Self::Inventory(InventorySourceError::Store(error))
    }
}
