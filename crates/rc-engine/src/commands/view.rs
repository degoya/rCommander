//! How a panel lists its entries: `panel.sort` (Ctrl+F3 to F6 or a click on a column header),
//! `panel.set_view` (Ctrl+F1 brief, Ctrl+F2 full) and `panel.toggle_hidden` (Total Commander's
//! `cm_SwitchHidSys`, no standard key), whose choice is stored in the database.

use std::sync::Arc;

use crate::{
    Engine, EngineError, EngineEvent, ListMode, Outcome, PanelChange, PanelId, SortKey, SortOrder,
    settings::{SORT_NAMES_KEY, SORT_NUMBERS_KEY, stored_setting},
    sort::{NameOrder, names_for, set_names, sort_entries},
};

/// The setting that keeps `panel.toggle_hidden`'s choice across restarts.
pub(crate) const SHOW_HIDDEN_KEY: &str = "panels.show_hidden";

impl Engine {
    /// `panel.sort`: the entries sorted again, outside the lock; the cursor stays on its entry.
    /// A quick filter in force stays: the whole listing is sorted and filtered again.
    pub(crate) fn sort(
        &self,
        panel: Option<PanelId>,
        key: SortKey,
        descending: Option<bool>,
    ) -> Outcome {
        let id = self.resolve(panel);
        let (entries, order, names) = {
            let mut state = self.state();
            let panel = state.panel_mut(id);
            let order = match descending {
                Some(descending) => SortOrder { key, descending },
                None => panel.sort.choose(key),
            };
            panel.sort = order;
            (panel.listed(), order, names_for(&panel.path))
        };
        let mut sorted = entries.to_vec();
        sort_entries(&mut sorted, order, names);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            // A listing that arrived meanwhile is sorted already, and a later sort wins.
            if Arc::ptr_eq(&panel.listed(), &entries) && panel.sort == order {
                let name = panel.cursor_entry().map(|entry| entry.name.clone());
                panel.relist(sorted.into());
                panel.cursor = name.and_then(|name| panel.index_of(&name)).unwrap_or(0);
            }
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
        Outcome::Panel(snapshot)
    }

    /// `panel.set_view`.
    pub(crate) fn set_list_mode(&self, panel: Option<PanelId>, mode: ListMode) -> Outcome {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            panel.mode = mode;
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
        Outcome::Panel(snapshot)
    }

    /// `panel.toggle_hidden`: both panels read again with or without their hidden and system
    /// entries; the choice is stored when a database is attached.
    pub(crate) async fn toggle_hidden(&self, show: Option<bool>) -> Result<Outcome, EngineError> {
        let show = show.unwrap_or_else(|| !self.state().show_hidden);
        if let Ok(db) = self.db()
            && let Err(error) = db.set(SHOW_HIDDEN_KEY, &show).await
        {
            // The panels change anyway; only the next start forgets it.
            tracing::warn!(%error, "the hidden-files setting was not stored");
        }
        self.show_hidden(show).await?;
        Ok(Outcome::Panel(self.snapshot(None)))
    }

    /// Applies the stored listing settings once the database is attached: the name order, the
    /// archives' decoder memory, hidden entries shown when the last run showed them. A setting
    /// that cannot be read leaves the defaults.
    pub async fn restore_settings(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        self.read_name_order().await;
        self.read_archive_limits().await;
        if let Ok(Some(true)) = db.get::<bool>(SHOW_HIDDEN_KEY).await {
            // A panel that cannot be read again keeps its listing and says so on the next key.
            if let Err(error) = self.show_hidden(true).await {
                tracing::debug!(
                    code = error.code(),
                    "a panel was not read again with hidden files"
                );
            }
        }
    }

    /// `sort.names` and `sort.numbers_by_value` as stored, for every sort from now on; the
    /// defaults where nothing (valid) is.
    pub(crate) async fn read_name_order(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        let order = stored_setting(db, SORT_NAMES_KEY).await;
        let order = NameOrder::from_setting(order.as_ref().and_then(|v| v.as_str()).unwrap_or(""));
        let numbers = stored_setting(db, SORT_NUMBERS_KEY).await;
        set_names(order, numbers.and_then(|v| v.as_bool()).unwrap_or(true));
    }

    /// A sort setting changed: it holds from now on, and both panels sort again by reading their
    /// folders again; a panel that cannot be read keeps its listing and says so on the next key.
    pub(crate) async fn name_order_changed(&self) {
        self.read_name_order().await;
        for panel in PanelId::BOTH {
            if let Err(error) = self.refresh(Some(panel)).await {
                tracing::debug!(code = error.code(), "a panel was not sorted again");
            }
        }
    }

    /// Both panels listed again with `show` for hidden entries; the first failure, after both
    /// were tried.
    async fn show_hidden(&self, show: bool) -> Result<(), EngineError> {
        self.state().show_hidden = show;
        let mut failure = None;
        for panel in PanelId::BOTH {
            if let Err(error) = self.refresh(Some(panel)).await {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
