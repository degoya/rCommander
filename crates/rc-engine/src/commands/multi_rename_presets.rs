//! The multi-rename presets (RC-020-02): Total Commander's saved settings of the multi-rename
//! tool, each a [`RenameRule`] under a name, and the rule of the last run, which
//! `multi_rename.apply` remembers by itself.
//!
//! Both are settings values (`multi_rename.presets`, `multi_rename.last`), not a table: a few
//! dozen small rules, durable, never growing with use beyond [`MAX_PRESETS`]. What is stored is
//! read back as untrusted - a preset with a bad name or an overlong text is dropped - and every
//! change reads, changes and writes the list under one lock, so two saves never lose each other.

use std::sync::OnceLock;

use rc_rename::RenameRule;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{Engine, EngineError, EngineEvent, Outcome, PanelId};

/// The setting the presets are kept under.
pub const PRESETS_KEY: &str = "multi_rename.presets";

/// The setting the rule of the last run is kept under.
pub const LAST_RULE_KEY: &str = "multi_rename.last";

/// The most presets kept.
pub const MAX_PRESETS: usize = 200;

/// The longest preset name, in characters.
pub const MAX_PRESET_NAME: usize = 64;

/// A saved multi-rename rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenamePreset {
    /// Unique among the presets, compared without regard to case.
    pub name: String,
    pub rule: RenameRule,
}

/// Held while the presets are read, changed and written.
fn presets_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// `name` trimmed, or why it cannot name a preset.
fn preset_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    let reason = if name.is_empty() {
        "the name is empty"
    } else if name.chars().count() > MAX_PRESET_NAME {
        "the name is longer than 64 characters"
    } else if name.chars().any(char::is_control) {
        "the name contains a control character"
    } else {
        return Ok(name.to_owned());
    };
    Err(EngineError::RenamePresetInvalid {
        reason: reason.to_owned(),
    })
}

/// Whether two preset names are the same one.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The stored presets as far as they can be trusted: valid names, bounded rules, no name twice,
/// at most [`MAX_PRESETS`], sorted by name.
pub(crate) fn sanitized(stored: Vec<RenamePreset>) -> Vec<RenamePreset> {
    let mut presets: Vec<RenamePreset> = Vec::with_capacity(stored.len().min(MAX_PRESETS));
    for preset in stored {
        let Ok(name) = preset_name(&preset.name) else {
            continue;
        };
        let taken = presets.iter().any(|kept| same_name(&kept.name, &name));
        if !taken && preset.rule.check().is_ok() && presets.len() < MAX_PRESETS {
            presets.push(RenamePreset {
                name,
                rule: preset.rule,
            });
        }
    }
    presets.sort_by_key(|preset| preset.name.to_lowercase());
    presets
}

impl Engine {
    /// `multi_rename.open_dialog`: announces that the dialog is wanted for the panel.
    pub(crate) fn multi_rename_dialog(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        self.publish(EngineEvent::MultiRenameDialog { panel: id });
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// `multi_rename.preset_list`.
    pub(crate) async fn rename_presets(&self) -> Result<Outcome, EngineError> {
        let presets = self.stored_presets().await?;
        self.presets_outcome(presets).await
    }

    /// `multi_rename.preset_save`: `rule` under `name`, in place of a preset of that name.
    pub(crate) async fn save_rename_preset(
        &self,
        name: &str,
        rule: &RenameRule,
    ) -> Result<Outcome, EngineError> {
        let name = preset_name(name)?;
        rule.check()?;
        let _guard = presets_lock().lock().await;
        let mut presets = self.stored_presets().await?;
        let preset = RenamePreset {
            name,
            rule: rule.clone(),
        };
        match presets
            .iter()
            .position(|kept| same_name(&kept.name, &preset.name))
        {
            Some(index) => presets[index] = preset,
            None if presets.len() >= MAX_PRESETS => {
                return Err(EngineError::RenamePresetInvalid {
                    reason: format!("at most {MAX_PRESETS} presets are kept"),
                });
            }
            None => presets.push(preset),
        }
        self.store_presets(presets).await
    }

    /// `multi_rename.preset_delete`.
    pub(crate) async fn delete_rename_preset(&self, name: &str) -> Result<Outcome, EngineError> {
        let _guard = presets_lock().lock().await;
        let mut presets = self.stored_presets().await?;
        let before = presets.len();
        presets.retain(|preset| !same_name(&preset.name, name.trim()));
        if presets.len() == before {
            return Err(EngineError::RenamePresetNotFound {
                name: name.to_owned(),
            });
        }
        self.store_presets(presets).await
    }

    /// `multi_rename.preset_rename`: refused when another preset has `new_name` already.
    pub(crate) async fn rename_rename_preset(
        &self,
        name: &str,
        new_name: &str,
    ) -> Result<Outcome, EngineError> {
        let new_name = preset_name(new_name)?;
        let _guard = presets_lock().lock().await;
        let mut presets = self.stored_presets().await?;
        let Some(index) = presets
            .iter()
            .position(|preset| same_name(&preset.name, name.trim()))
        else {
            return Err(EngineError::RenamePresetNotFound {
                name: name.to_owned(),
            });
        };
        let clash = presets
            .iter()
            .enumerate()
            .any(|(other, preset)| other != index && same_name(&preset.name, &new_name));
        if clash {
            return Err(EngineError::RenamePresetInvalid {
                reason: format!("a preset named {new_name} exists already"),
            });
        }
        presets[index].name = new_name;
        self.store_presets(presets).await
    }

    async fn stored_presets(&self) -> Result<Vec<RenamePreset>, EngineError> {
        let stored: Option<Vec<RenamePreset>> = self.db()?.get(PRESETS_KEY).await?;
        Ok(sanitized(stored.unwrap_or_default()))
    }

    async fn store_presets(&self, presets: Vec<RenamePreset>) -> Result<Outcome, EngineError> {
        let presets = sanitized(presets);
        self.db()?.set(PRESETS_KEY, &presets).await?;
        self.presets_outcome(presets).await
    }

    /// The presets with the rule last used; a last rule that does not read is left out.
    async fn presets_outcome(&self, presets: Vec<RenamePreset>) -> Result<Outcome, EngineError> {
        let last = self
            .db()?
            .get::<RenameRule>(LAST_RULE_KEY)
            .await
            .ok()
            .flatten()
            .filter(|rule| rule.check().is_ok());
        Ok(Outcome::RenamePresets { presets, last })
    }
}

#[cfg(test)]
#[path = "multi_rename_presets_tests.rs"]
mod tests;
