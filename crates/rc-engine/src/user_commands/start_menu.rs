//! The Start menu (RC-100-86): `start_menu.list`, `.save`, `.move` and `.delete` over `rc_db`'s
//! `start_menu_items`. `rc_db` holds the tree to its limits (512 rows, 8 levels, a submenu as the
//! only parent); this module checks the rows ([`StartItemDraft::checked`]) and gives the tree back
//! in menu order, each submenu before its rows. A row that does not read is left out with a
//! warning, and so is everything under a submenu left out. Every change announces
//! [`EngineEvent::StartMenuChanged`].

use std::collections::HashMap;

use rc_db::{CommandFields, StartMenuRecord, StoredStartMenuItem};

use super::{
    StartItem, StartItemDraft, StartItemId, StartItemKind, UserCommandError, UserCommandsOutcome,
    answer,
    store::{action_of, fields_of},
    store_error,
};
use crate::{Engine, EngineError, EngineEvent, Outcome};

/// The rows of `stored` in menu order with their depth: the top level in its order, each
/// submenu followed by its rows. Rows whose parent is not reached (left out, or a loop edited
/// into the file) are never reached either.
pub(super) fn menu_order(stored: Vec<StoredStartMenuItem>) -> Vec<StartItem> {
    let mut children: HashMap<Option<i64>, Vec<StoredStartMenuItem>> = HashMap::new();
    for row in stored {
        children.entry(row.parent_id).or_default().push(row);
    }
    for rows in children.values_mut() {
        rows.sort_by_key(|row| (row.position, row.id));
    }
    let mut items = Vec::new();
    // A stack of the levels still to walk, each with its depth; the deepest on top.
    let mut stack: Vec<(usize, std::vec::IntoIter<StoredStartMenuItem>)> = Vec::new();
    if let Some(top) = children.remove(&None) {
        stack.push((0, top.into_iter()));
    }
    while let Some((depth, rows)) = stack.last_mut() {
        let depth = *depth;
        let Some(row) = rows.next() else {
            stack.pop();
            continue;
        };
        let id = row.id;
        let Some(item) = item_of(row, depth) else {
            tracing::warn!(id, "a stored Start menu row does not read and is left out");
            continue;
        };
        let submenu = item.kind == StartItemKind::Submenu;
        items.push(item);
        if submenu
            && depth + 1 < rc_db::START_MENU_MAX_DEPTH
            && let Some(below) = children.remove(&Some(id))
        {
            stack.push((depth + 1, below.into_iter()));
        }
    }
    items
}

/// One stored row at `depth`; `None` when it does not read.
fn item_of(row: StoredStartMenuItem, depth: usize) -> Option<StartItem> {
    let kind = StartItemKind::parse(&row.record.kind)?;
    let action = match kind {
        StartItemKind::Item => Some(action_of(&row.record.fields)?),
        StartItemKind::Submenu | StartItemKind::Separator => None,
    };
    Some(StartItem {
        id: row.id,
        parent: row.parent_id,
        depth,
        kind,
        title: row.record.title,
        action,
    })
}

/// `draft` (checked) as the database keeps it.
fn record_of(draft: &StartItemDraft) -> StartMenuRecord {
    StartMenuRecord {
        kind: draft.kind.as_str().to_owned(),
        title: draft.title.clone(),
        fields: draft
            .action
            .as_ref()
            .map_or_else(CommandFields::default, fields_of),
    }
}

impl Engine {
    /// The Start menu in menu order.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails.
    pub async fn start_menu(&self) -> Result<Vec<StartItem>, EngineError> {
        let stored = self.db()?.start_menu().await.map_err(store_error)?;
        Ok(menu_order(stored))
    }

    /// The entry `id` as the menu shows it.
    pub(crate) async fn start_item(&self, id: StartItemId) -> Result<StartItem, EngineError> {
        let stored = self.db()?.start_menu_item(id).await.map_err(store_error)?;
        item_of(stored, 0).ok_or_else(|| {
            UserCommandError::StartInvalid {
                reason: "the row does not read".to_owned(),
            }
            .into()
        })
    }

    /// `start_menu.list`.
    pub(crate) async fn list_start_menu(&self) -> Result<Outcome, EngineError> {
        let items = self.start_menu().await?;
        Ok(answer(UserCommandsOutcome::StartMenu { items }))
    }

    /// `start_menu.save`.
    pub(crate) async fn save_start_item(
        &self,
        id: Option<StartItemId>,
        parent: Option<StartItemId>,
        draft: StartItemDraft,
    ) -> Result<Outcome, EngineError> {
        let id = self.store_start_item(id, parent, &draft).await?;
        Ok(answer(UserCommandsOutcome::Saved { id }))
    }

    /// Stores `draft` as a new row after the last one of `parent`, or over `id` in its place,
    /// checked; the id it has. The import of Total Commander's Start menu stores through here.
    pub(crate) async fn store_start_item(
        &self,
        id: Option<StartItemId>,
        parent: Option<StartItemId>,
        draft: &StartItemDraft,
    ) -> Result<StartItemId, EngineError> {
        let record = record_of(&draft.checked()?);
        let db = self.db()?;
        let stored = match id {
            Some(id) => db.update_start_menu_item(id, &record).await,
            None => db.add_start_menu_item(parent, &record).await,
        }
        .map_err(store_error)?;
        self.publish(EngineEvent::StartMenuChanged);
        Ok(stored.id)
    }

    /// `start_menu.move`.
    pub(crate) async fn move_start_item(
        &self,
        id: StartItemId,
        parent: Option<StartItemId>,
        position: Option<usize>,
    ) -> Result<Outcome, EngineError> {
        self.db()?
            .move_start_menu_item(id, parent, position)
            .await
            .map_err(store_error)?;
        self.publish(EngineEvent::StartMenuChanged);
        Ok(answer(UserCommandsOutcome::Done {}))
    }

    /// `start_menu.delete`: a submenu with everything in it.
    pub(crate) async fn delete_start_item(&self, id: StartItemId) -> Result<Outcome, EngineError> {
        let removed = self
            .db()?
            .delete_start_menu_item(id)
            .await
            .map_err(store_error)?;
        self.publish(EngineEvent::StartMenuChanged);
        Ok(answer(UserCommandsOutcome::Deleted { removed }))
    }
}

#[cfg(test)]
#[path = "start_menu_tests.rs"]
mod tests;
