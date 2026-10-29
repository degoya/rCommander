//! The directory hotlist (RC-010-03): Total Commander's Ctrl+D menu of favourite folders, with
//! folders (submenus) and, per entry, an optional folder for the other panel.
//!
//! The rows live in `rc-db` (`hotlist`); locations are stored as `VfsPath` JSON, so an entry on
//! a server or inside an archive needs no new column. A change is announced as
//! [`EngineEvent::HotlistChanged`]; the window reads the list again.

use std::collections::HashMap;

use rc_db::{HotlistItem, HotlistPatch, NewHotlistItem};
use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, EngineEvent, Outcome,
    commands::resolve_path,
    tabs::{
        TabsError,
        requests::{
            AddHotlistFolderRequest, AddHotlistRequest, MoveHotlistRequest, OpenHotlistRequest,
            UpdateHotlistRequest,
        },
        title::last_part,
    },
};

/// The longest location stored, as JSON bytes.
const MAX_LOCATION: usize = 8 * 1024;

/// A hotlist row as commands report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotlistEntry {
    pub id: i64,
    /// The hotlist folder it is in; `None` at the top.
    pub parent_id: Option<i64>,
    pub name: String,
    /// A hotlist folder (a submenu), not an entry.
    pub folder: bool,
    /// The folder the entry opens; `None` for a hotlist folder or a row that does not read.
    pub path: Option<VfsPath>,
    /// What the other panel shows with it.
    pub target: Option<VfsPath>,
}

impl HotlistEntry {
    fn from_item(item: HotlistItem) -> Self {
        let read = |json: Option<&String>| json.and_then(|json| serde_json::from_str(json).ok());
        Self {
            id: item.id,
            parent_id: item.parent_id,
            folder: item.is_folder(),
            path: read(item.location.as_ref()),
            target: read(item.target.as_ref()),
            name: item.name,
        }
    }
}

/// `items` (as `rc-db` lists them, level by level) in menu order: each folder followed by what
/// it holds, depth first, each level in its order. One pass to find each folder's items, one to
/// write them out: an item whose folder is not among them is left out.
fn menu_order(items: Vec<HotlistItem>) -> Vec<HotlistEntry> {
    let mut held: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        held.entry(item.parent_id).or_default().push(index);
    }
    let mut slots: Vec<Option<HotlistItem>> = items.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(slots.len());
    let below = |parent: Option<i64>| held.get(&parent).into_iter().flatten().rev().copied();
    let mut stack: Vec<usize> = below(None).collect();
    while let Some(index) = stack.pop() {
        // Each item once, whatever its folders say.
        let Some(item) = slots[index].take() else {
            continue;
        };
        stack.extend(below(Some(item.id)));
        out.push(HotlistEntry::from_item(item));
    }
    out
}

/// `path` as stored.
fn location(path: &VfsPath) -> Result<String, EngineError> {
    serde_json::to_string(path)
        .ok()
        .filter(|json| json.len() <= MAX_LOCATION)
        .ok_or_else(|| TabsError::HotlistBadPath.into())
}

impl Engine {
    /// `hotlist.list`: every row in menu order.
    pub(crate) async fn list_hotlist(&self) -> Result<Outcome, EngineError> {
        let items = self.db()?.hotlist().await?;
        Ok(Outcome::Hotlist {
            items: menu_order(items),
        })
    }

    /// `hotlist.add`: an entry for the panel's folder unless a path is given.
    pub(crate) async fn add_hotlist(
        &self,
        request: AddHotlistRequest,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let (here, there) = {
            let state = self.state();
            (
                state.panel(id).path.clone(),
                state.panel(id.other()).path.clone(),
            )
        };
        let path = match &request.path {
            Some(path) => resolve_path(&here, path),
            None => here,
        };
        let target = request
            .target
            .as_deref()
            .map(|target| location(&resolve_path(&there, target)))
            .transpose()?;
        let name = request.name.unwrap_or_else(|| last_part(&path));
        let stored = location(&path)?;
        let item = self
            .db()?
            .add_hotlist(&NewHotlistItem {
                parent_id: request.parent_id,
                name: &name,
                location: Some(&stored),
                target: target.as_deref(),
            })
            .await?;
        Ok(self.hotlist_changed(item))
    }

    /// `hotlist.add_folder`.
    pub(crate) async fn add_hotlist_folder(
        &self,
        request: AddHotlistFolderRequest,
    ) -> Result<Outcome, EngineError> {
        let item = self
            .db()?
            .add_hotlist(&NewHotlistItem {
                parent_id: request.parent_id,
                name: &request.name,
                location: None,
                target: None,
            })
            .await?;
        Ok(self.hotlist_changed(item))
    }

    /// `hotlist.update`.
    pub(crate) async fn update_hotlist(
        &self,
        request: UpdateHotlistRequest,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let (here, there) = {
            let state = self.state();
            (
                state.panel(id).path.clone(),
                state.panel(id.other()).path.clone(),
            )
        };
        let path = request
            .path
            .as_deref()
            .map(|path| location(&resolve_path(&here, path)))
            .transpose()?;
        let target = if request.clear_target {
            Some(None)
        } else {
            request
                .target
                .as_deref()
                .map(|target| location(&resolve_path(&there, target)).map(Some))
                .transpose()?
        };
        let patch = HotlistPatch {
            name: request.name,
            location: path,
            target,
        };
        let item = self.db()?.update_hotlist(request.id, &patch).await?;
        Ok(self.hotlist_changed(item))
    }

    /// `hotlist.move`.
    pub(crate) async fn move_hotlist(
        &self,
        request: MoveHotlistRequest,
    ) -> Result<Outcome, EngineError> {
        let item = self
            .db()?
            .move_hotlist(request.id, request.parent_id, request.position)
            .await?;
        Ok(self.hotlist_changed(item))
    }

    /// `hotlist.delete`: a folder goes with everything in it.
    pub(crate) async fn delete_hotlist(&self, id: i64) -> Result<Outcome, EngineError> {
        let removed = self.db()?.delete_hotlist(id).await?;
        self.publish(EngineEvent::HotlistChanged);
        Ok(Outcome::HotlistDeleted { id, removed })
    }

    /// `hotlist.open`: the panel shows the entry's folder, the other panel its target.
    pub(crate) async fn open_hotlist(
        &self,
        request: OpenHotlistRequest,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let item = self.db()?.hotlist_item(request.id).await?;
        if item.is_folder() {
            return Err(TabsError::HotlistNotAnEntry { id: request.id }.into());
        }
        let entry = HotlistEntry::from_item(item);
        let path = entry.path.ok_or(TabsError::HotlistBadPath)?;
        let shown = self.show_path(id, path).await?;
        if let Some(target) = entry.target {
            self.show_path(id.other(), target).await?;
        }
        Ok(shown)
    }

    fn hotlist_changed(&self, item: HotlistItem) -> Outcome {
        self.publish(EngineEvent::HotlistChanged);
        Outcome::HotlistItem {
            item: HotlistEntry::from_item(item),
        }
    }
}

#[cfg(test)]
#[path = "hotlist_tests.rs"]
mod tests;
