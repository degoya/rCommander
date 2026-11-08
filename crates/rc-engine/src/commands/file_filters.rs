//! `panel.file_filter` and the saved filters (`filters.*`), Total Commander's Ctrl+F10, Ctrl+F11
//! and Ctrl+F12 (RC-100-92, `file_filter.rs`); and `panel.toggle_ignore_list`.
//!
//! The saved filters are a settings value (`filters.saved`), not a table, as the multi-rename
//! presets are: a few dozen masks under names, durable, at most [`MAX_SAVED_FILTERS`]. What is
//! stored is read back as untrusted - a filter with a bad name or mask is dropped - and every
//! change reads, changes and writes the list under one lock.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, TabsError,
    file_filter::{FILTER_PER_TAB_KEY, FileFilter},
    ignore_list::IGNORE_ENABLED_KEY,
};

/// The setting the saved filters are kept under.
pub const SAVED_FILTERS_KEY: &str = "filters.saved";

/// The most saved filters kept.
pub const MAX_SAVED_FILTERS: usize = 50;

/// The longest name of a saved filter, in characters.
pub const MAX_FILTER_NAME: usize = 64;

/// A file filter's mask under a name (Ctrl+F12's list).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedFilter {
    /// Unique among the saved filters, compared without regard to case.
    pub name: String,
    pub mask: String,
}

/// Held while the saved filters are read, changed and written.
fn filters_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// `name` trimmed, or why it cannot name a saved filter.
fn filter_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    let reason = if name.is_empty() {
        "the name is empty"
    } else if name.chars().count() > MAX_FILTER_NAME {
        "the name is longer than 64 characters"
    } else if name.chars().any(char::is_control) {
        "the name contains a control character"
    } else {
        return Ok(name.to_owned());
    };
    Err(TabsError::SavedFilterInvalid { reason }.into())
}

/// `mask` as a saved filter keeps it, or why not: not blank, as a file filter takes it.
fn filter_mask(mask: &str) -> Result<String, EngineError> {
    match FileFilter::Mask(mask.to_owned()).checked()? {
        FileFilter::Mask(mask) => Ok(mask),
        _ => Err(TabsError::SavedFilterInvalid {
            reason: "the mask is empty",
        }
        .into()),
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The stored filters as far as they can be trusted: valid names and masks, no name twice, at
/// most [`MAX_SAVED_FILTERS`], sorted by name.
pub(crate) fn sanitized(stored: Vec<SavedFilter>) -> Vec<SavedFilter> {
    let mut kept: Vec<SavedFilter> = Vec::with_capacity(stored.len().min(MAX_SAVED_FILTERS));
    for filter in stored {
        let (Ok(name), Ok(mask)) = (filter_name(&filter.name), filter_mask(&filter.mask)) else {
            continue;
        };
        let taken = kept.iter().any(|known| same_name(&known.name, &name));
        if !taken && kept.len() < MAX_SAVED_FILTERS {
            kept.push(SavedFilter { name, mask });
        }
    }
    kept.sort_by_key(|filter| filter.name.to_lowercase());
    kept
}

impl Engine {
    /// `panel.file_filter`: the panel shows the files `filter` takes, folders always; `all` lifts
    /// the quick filter too (Ctrl+F10). The active tab keeps the filter, or with
    /// `tabs.filter_per_tab` off every tab of the panel.
    pub(crate) async fn set_file_filter(
        &self,
        panel: Option<PanelId>,
        filter: FileFilter,
    ) -> Result<Outcome, EngineError> {
        let filter = filter.checked()?;
        let per_tab = self.setting_on(FILTER_PER_TAB_KEY).await;
        let id = self.resolve(panel);
        let snapshot = {
            let mut guard = self.state();
            let state = &mut *guard;
            let active = state.active == id;
            if !per_tab {
                state.tabs.filter_all(id, &filter);
            }
            let panel = state.panel_mut(id);
            if filter.is_all() {
                panel.filter.pattern = None;
            }
            panel.set_file_filter(filter);
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
        Ok(Outcome::Panel(snapshot))
    }

    /// `filters.list`.
    pub(crate) async fn list_file_filters(&self) -> Result<Outcome, EngineError> {
        Ok(Outcome::FileFilters {
            saved: self.stored_filters().await?,
        })
    }

    /// `filters.save`: `mask` under `name`, in place of a filter of that name.
    pub(crate) async fn save_file_filter(
        &self,
        name: &str,
        mask: &str,
    ) -> Result<Outcome, EngineError> {
        let filter = SavedFilter {
            name: filter_name(name)?,
            mask: filter_mask(mask)?,
        };
        let _guard = filters_lock().lock().await;
        let mut saved = self.stored_filters().await?;
        match saved
            .iter()
            .position(|kept| same_name(&kept.name, &filter.name))
        {
            Some(index) => saved[index] = filter,
            None if saved.len() >= MAX_SAVED_FILTERS => {
                return Err(TabsError::SavedFilterInvalid {
                    reason: "at most 50 filters are kept",
                }
                .into());
            }
            None => saved.push(filter),
        }
        self.store_filters(saved).await
    }

    /// `filters.rename`: refused when another filter has `new_name` already.
    pub(crate) async fn rename_file_filter(
        &self,
        name: &str,
        new_name: &str,
    ) -> Result<Outcome, EngineError> {
        let new_name = filter_name(new_name)?;
        let _guard = filters_lock().lock().await;
        let mut saved = self.stored_filters().await?;
        let index = saved
            .iter()
            .position(|filter| same_name(&filter.name, name.trim()))
            .ok_or_else(|| TabsError::SavedFilterNotFound {
                name: name.to_owned(),
            })?;
        let clash = saved
            .iter()
            .enumerate()
            .any(|(other, filter)| other != index && same_name(&filter.name, &new_name));
        if clash {
            return Err(TabsError::SavedFilterInvalid {
                reason: "another filter has that name",
            }
            .into());
        }
        saved[index].name = new_name;
        self.store_filters(saved).await
    }

    /// `filters.delete`.
    pub(crate) async fn delete_file_filter(&self, name: &str) -> Result<Outcome, EngineError> {
        let _guard = filters_lock().lock().await;
        let mut saved = self.stored_filters().await?;
        let before = saved.len();
        saved.retain(|filter| !same_name(&filter.name, name.trim()));
        if saved.len() == before {
            return Err(TabsError::SavedFilterNotFound {
                name: name.to_owned(),
            }
            .into());
        }
        self.store_filters(saved).await
    }

    async fn stored_filters(&self) -> Result<Vec<SavedFilter>, EngineError> {
        let stored: Option<Vec<SavedFilter>> = self.db()?.get(SAVED_FILTERS_KEY).await?;
        Ok(sanitized(stored.unwrap_or_default()))
    }

    async fn store_filters(&self, saved: Vec<SavedFilter>) -> Result<Outcome, EngineError> {
        let saved = sanitized(saved);
        self.db()?.set(SAVED_FILTERS_KEY, &saved).await?;
        Ok(Outcome::FileFilters { saved })
    }

    /// `panel.toggle_ignore_list`: `ignore_list.enabled` switched, or set to `enabled`, as the
    /// settings dialog sets it; both panels are read again.
    pub(crate) async fn toggle_ignore_list(
        &self,
        enabled: Option<bool>,
    ) -> Result<Outcome, EngineError> {
        let on = enabled.unwrap_or(!self.setting_on(IGNORE_ENABLED_KEY).await);
        let settings = self
            .set_protected_setting(IGNORE_ENABLED_KEY, &serde_json::Value::Bool(on))
            .await?;
        Ok(Outcome::Settings {
            settings: vec![settings],
        })
    }
}

#[cfg(test)]
#[path = "file_filters_tests.rs"]
mod tests;
