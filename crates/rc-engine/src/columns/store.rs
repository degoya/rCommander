//! The stored column sets (RC-100-90): `column_sets.list`, `.get`, `.create`, `.update`,
//! `.rename`, `.move` and `.delete` over `rc_db`'s `column_sets` table.
//!
//! What goes in is checked here as at any boundary: the name ([`clean_name`], unique without
//! regard to case), the spec ([`ColumnSetSpec::to_json`]), the count ([`MAX_SETS`]). What comes
//! back is read as untrusted too: a row whose spec no longer reads is left out of the list with a
//! warning. Every change announces [`EngineEvent::ColumnSetsChanged`]; a set deleted while a
//! panel or a tab shows it leaves the full view there.

use rc_db::{DbError, StoredColumnSet};

use super::{
    ColumnSet, ColumnSetError, ColumnSetId, ColumnSetSpec, ColumnSetsOutcome, MAX_SETS, clean_name,
};
use crate::{Engine, EngineError, EngineEvent, ListMode, Outcome, PanelChange, PanelId};

/// A `column_sets.*` answer.
fn answer(outcome: ColumnSetsOutcome) -> Outcome {
    Outcome::ColumnSets(Box::new(outcome))
}

/// The row `stored` as a column set.
fn set_of(stored: StoredColumnSet) -> Result<ColumnSet, ColumnSetError> {
    Ok(ColumnSet {
        spec: ColumnSetSpec::parse(&stored.spec)?,
        id: stored.id,
        name: stored.name,
    })
}

/// A failed write: a name another set has, or the database's.
fn write_error(error: DbError, name: &str) -> EngineError {
    if error.is_unique_violation() {
        ColumnSetError::NameTaken {
            name: name.to_owned(),
        }
        .into()
    } else {
        EngineError::Database(error)
    }
}

impl Engine {
    /// Every column set in the person's order; a row whose spec does not read is left out with a
    /// warning.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails.
    pub async fn column_sets(&self) -> Result<Vec<ColumnSet>, EngineError> {
        let stored = self.db()?.column_sets().await?;
        Ok(stored
            .into_iter()
            .filter_map(|row| {
                let id = row.id;
                set_of(row)
                    .map_err(|error| {
                        tracing::warn!(id, code = error.code(), "a stored column set is left out");
                    })
                    .ok()
            })
            .collect())
    }

    /// The column set `id`, read and its spec parsed.
    pub(crate) async fn read_column_set(&self, id: ColumnSetId) -> Result<ColumnSet, EngineError> {
        let stored = self
            .db()?
            .column_set(id)
            .await?
            .ok_or(ColumnSetError::Unknown { id })?;
        Ok(set_of(stored)?)
    }

    /// `column_sets.list`.
    pub(crate) async fn list_column_sets(&self) -> Result<Outcome, EngineError> {
        let sets = self.column_sets().await?;
        Ok(answer(ColumnSetsOutcome::List { sets }))
    }

    /// `column_sets.get`.
    pub(crate) async fn get_column_set(&self, id: ColumnSetId) -> Result<Outcome, EngineError> {
        let set = self.read_column_set(id).await?;
        Ok(answer(ColumnSetsOutcome::Set(Box::new(set))))
    }

    /// `column_sets.create`: a new set after the last one.
    pub(crate) async fn create_column_set(
        &self,
        name: &str,
        spec: &ColumnSetSpec,
    ) -> Result<Outcome, EngineError> {
        let name = clean_name(name)?;
        let spec = spec.to_json()?;
        let db = self.db()?;
        if db.column_sets().await?.len() >= MAX_SETS {
            return Err(ColumnSetError::TooMany.into());
        }
        let stored = db
            .create_column_set(&name, &spec)
            .await
            .map_err(|error| write_error(error, &name))?;
        self.publish(EngineEvent::ColumnSetsChanged);
        Ok(answer(ColumnSetsOutcome::Saved { id: stored.id }))
    }

    /// `column_sets.update` (a new spec) and `column_sets.rename` (a new name).
    pub(crate) async fn update_column_set(
        &self,
        id: ColumnSetId,
        name: Option<&str>,
        spec: Option<&ColumnSetSpec>,
    ) -> Result<Outcome, EngineError> {
        let name = name.map(clean_name).transpose()?;
        let spec = spec.map(ColumnSetSpec::to_json).transpose()?;
        let shown = name.clone().unwrap_or_default();
        self.db()?
            .update_column_set(id, name.as_deref(), spec.as_deref())
            .await
            .map_err(|error| write_error(error, &shown))?
            .ok_or(ColumnSetError::Unknown { id })?;
        self.publish(EngineEvent::ColumnSetsChanged);
        // A panel showing the set draws its new columns, and reads comments it now shows.
        for panel in self.panels_showing(id) {
            self.columns_follow(panel, false).await;
        }
        Ok(answer(ColumnSetsOutcome::Done {}))
    }

    /// `column_sets.move`.
    pub(crate) async fn move_column_set(
        &self,
        id: ColumnSetId,
        position: usize,
    ) -> Result<Outcome, EngineError> {
        if !self.db()?.move_column_set(id, position).await? {
            return Err(ColumnSetError::Unknown { id }.into());
        }
        self.publish(EngineEvent::ColumnSetsChanged);
        Ok(answer(ColumnSetsOutcome::Done {}))
    }

    /// `column_sets.delete`: a panel or a tab showing the set shows the full view.
    pub(crate) async fn delete_column_set(&self, id: ColumnSetId) -> Result<Outcome, EngineError> {
        if !self.db()?.delete_column_set(id).await? {
            return Err(ColumnSetError::Unknown { id }.into());
        }
        let showing = self.panels_showing(id);
        {
            let mut state = self.state();
            let gone = ListMode::Custom { set: id };
            for panel in PanelId::BOTH {
                if state.panel(panel).mode == gone {
                    let shown = state.panel_mut(panel);
                    shown.mode = ListMode::Full;
                    shown.columns.comments = None;
                }
                for tab in state.tabs.set_mut(panel).tabs_mut() {
                    if tab.mode == gone {
                        tab.mode = ListMode::Full;
                    }
                }
            }
        }
        self.publish(EngineEvent::ColumnSetsChanged);
        for panel in showing {
            self.publish(EngineEvent::Panel {
                panel,
                change: PanelChange::Listing,
            });
        }
        Ok(answer(ColumnSetsOutcome::Done {}))
    }

    /// The panels whose mode is the set `id`.
    fn panels_showing(&self, id: ColumnSetId) -> Vec<PanelId> {
        let state = self.state();
        PanelId::BOTH
            .into_iter()
            .filter(|panel| state.panel(*panel).mode == ListMode::Custom { set: id })
            .collect()
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
