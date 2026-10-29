//! The rows of an import's hotlist part (RC-080-19): the hotlist items and the tabs.
//!
//! Only places on this machine's file systems are read; one on a server or inside an archive is
//! refused (it would name another machine's connection). A place on a network share waits for
//! the person's tick, as does a device path: opening it hands the share the person's Windows
//! credentials (`rc_vfs::network_path`, the rule of RC-080-10). Nothing is opened to compare - an
//! item is the same as one here when it has the same name in the same folders (ignoring ASCII
//! case), a tab when its panel has a tab on that folder - so a second import adds nothing twice.
//! Tabs are added behind a panel's own.

use std::collections::{HashMap, HashSet};

use rc_db::{HOTLIST_MAX_DEPTH, HOTLIST_MAX_NAME, HotlistItem, HotlistPatch, NewHotlistItem};
use rc_vfs::{BackendId, VfsPath};

use super::{
    exchange_import::{choose, written},
    exchange_import_rows::{joined, refused, row},
};
use crate::{
    Engine, EngineError, EngineEvent, PanelId,
    exchange::{ExchangePart, ExchangeRow, ExportedHotlistItem, ExportedTab, RowStatus},
};

/// A name as the hotlist compares it.
fn folded(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Whether `place` is on this machine's file systems and reads as a path.
fn local(place: &VfsPath) -> bool {
    let text = place.path.to_string_lossy();
    place.backend == BackendId::Local
        && !text.is_empty()
        && text.chars().count() <= crate::exchange::MAX_TEXT
        && !text.contains('\0')
}

/// Whether opening `place` could reach another machine: a share in any spelling (`\\server`,
/// `//server`, `\/server`, `\\?\UNC\server`), a device or NT path (`\\.\pipe`, `\??\UNC`).
/// The rule of the terminal's and MCP's paths (RC-080-10, `rc_vfs::network_path`); a WSL
/// distribution's share is this machine.
pub(super) fn on_share(place: &VfsPath) -> bool {
    crate::network_path(&place.path.to_string_lossy()).is_some()
}

/// A place as the hotlist stores it.
fn stored(place: &VfsPath) -> Result<String, EngineError> {
    serde_json::to_string(place).map_err(|_| crate::tabs::TabsError::HotlistBadPath.into())
}

/// Whether the stored JSON `json` is `place`.
fn same_place(json: Option<&String>, place: Option<&VfsPath>) -> bool {
    let json: Option<VfsPath> = json.and_then(|json| serde_json::from_str(json).ok());
    json.as_ref() == place
}

/// The hotlist here: each item by its folders' names and its own (folded) and whether it is a
/// folder; each folder's id by its path.
struct Here {
    items: HashMap<(Vec<String>, String, bool), HotlistItem>,
    folders: HashMap<Vec<String>, i64>,
}

impl Here {
    fn of(items: Vec<HotlistItem>) -> Self {
        let parents: HashMap<i64, (Option<i64>, String)> = items
            .iter()
            .map(|item| (item.id, (item.parent_id, folded(&item.name))))
            .collect();
        let mut here = Self {
            items: HashMap::new(),
            folders: HashMap::new(),
        };
        for item in items {
            let mut path = Vec::new();
            let mut parent = item.parent_id;
            while let Some(id) = parent.filter(|_| path.len() <= HOTLIST_MAX_DEPTH) {
                let Some((above, name)) = parents.get(&id) else {
                    break;
                };
                path.insert(0, name.clone());
                parent = *above;
            }
            if item.is_folder() {
                let mut own = path.clone();
                own.push(folded(&item.name));
                here.folders.insert(own, item.id);
            }
            let key = (path, folded(&item.name), item.is_folder());
            here.items.entry(key).or_insert(item);
        }
        here
    }
}

impl Engine {
    /// The hotlist items against the hotlist.
    pub(super) async fn exchange_hotlist(
        &self,
        items: &[ExportedHotlistItem],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let db = self.db()?;
        let mut here = Here::of(db.hotlist().await?);
        let mut seen = HashSet::new();
        let mut rows = Vec::with_capacity(items.len());
        let mut changed = false;
        for (at, item) in items.iter().enumerate() {
            let folder = item.location.is_none();
            let mut name = joined(&item.folders, &item.name);
            if folder {
                name.push('/');
            }
            let mut row = row(format!("hotlist:{at}"), ExchangePart::Hotlist, name);
            row.new = item
                .location
                .as_ref()
                .map(|place| place.path.display().to_string());
            let path: Vec<String> = item.folders.iter().map(|name| folded(name)).collect();
            let places = [item.location.as_ref(), item.target.as_ref()];
            let valid = !folded(&item.name).is_empty()
                && item.name.chars().count() <= HOTLIST_MAX_NAME
                && path.len() < HOTLIST_MAX_DEPTH
                && !path.iter().any(String::is_empty)
                && places.into_iter().flatten().all(local)
                && (item.target.is_none() || !folder)
                && seen.insert((path.clone(), folded(&item.name), folder));
            if !valid {
                row.status = refused("exchange_file_invalid");
                rows.push(row);
                continue;
            }
            if places.into_iter().flatten().any(on_share) {
                row.warning = Some("network_location".to_owned());
            }
            let key = (path.clone(), folded(&item.name), folder);
            let there = here.items.get(&key).cloned();
            row.status = match &there {
                None => RowStatus::New,
                Some(_) if folder => RowStatus::Exists,
                Some(there)
                    if same_place(there.location.as_ref(), item.location.as_ref())
                        && same_place(there.target.as_ref(), item.target.as_ref()) =>
                {
                    RowStatus::Exists
                }
                Some(there) => {
                    row.old = there
                        .location
                        .as_deref()
                        .and_then(|json| serde_json::from_str::<VfsPath>(json).ok())
                        .map(|place| place.path.display().to_string());
                    RowStatus::Changed
                }
            };
            choose(&mut row, only);
            if !dry_run && row.selected {
                let result = self.write_hotlist_item(item, &path, there, &mut here).await;
                let done = if row.status == RowStatus::Changed {
                    RowStatus::Updated
                } else {
                    RowStatus::Created
                };
                row.status = written(result, done);
                changed = true;
            }
            rows.push(row);
        }
        if changed {
            self.publish(EngineEvent::HotlistChanged);
        }
        Ok(rows)
    }

    /// Writes one chosen item: a folder made with those above it, an entry added or updated.
    async fn write_hotlist_item(
        &self,
        item: &ExportedHotlistItem,
        path: &[String],
        there: Option<HotlistItem>,
        here: &mut Here,
    ) -> Result<(), EngineError> {
        let db = self.db()?;
        let Some(location) = &item.location else {
            let mut names = item.folders.clone();
            names.push(item.name.clone());
            let mut own = path.to_vec();
            own.push(folded(&item.name));
            self.hotlist_path(&names, &own, here).await?;
            return Ok(());
        };
        let location = stored(location)?;
        let target = item.target.as_ref().map(stored).transpose()?;
        match there {
            Some(there) => {
                let patch = HotlistPatch {
                    name: None,
                    location: Some(location),
                    target: Some(target),
                };
                db.update_hotlist(there.id, &patch).await?;
            }
            None => {
                let parent = self.hotlist_path(&item.folders, path, here).await?;
                db.add_hotlist(&NewHotlistItem {
                    parent_id: parent,
                    name: item.name.trim(),
                    location: Some(&location),
                    target: target.as_deref(),
                })
                .await?;
            }
        }
        Ok(())
    }

    /// The hotlist folder `names` (folded as `path`), made with those above it where missing;
    /// `None` for the top.
    async fn hotlist_path(
        &self,
        names: &[String],
        path: &[String],
        here: &mut Here,
    ) -> Result<Option<i64>, EngineError> {
        let mut parent = None;
        for depth in 1..=path.len() {
            if let Some(id) = here.folders.get(&path[..depth]) {
                parent = Some(*id);
                continue;
            }
            let made = self
                .db()?
                .add_hotlist(&NewHotlistItem {
                    parent_id: parent,
                    name: names[depth - 1].trim(),
                    location: None,
                    target: None,
                })
                .await?;
            here.folders.insert(path[..depth].to_vec(), made.id);
            parent = Some(made.id);
        }
        Ok(parent)
    }

    /// The tabs against the panels' tabs; the chosen ones added behind each panel's own.
    pub(super) async fn exchange_tabs(
        &self,
        tabs: &[ExportedTab],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let mut rows = Vec::with_capacity(tabs.len());
        let mut seen = HashSet::new();
        for (at, tab) in tabs.iter().enumerate() {
            let side = match tab.panel {
                PanelId::Left => "left",
                PanelId::Right => "right",
            };
            let mut row = row(
                format!("tab:{at}"),
                ExchangePart::Hotlist,
                format!("{side}: {}", tab.location.path.display()),
            );
            if !local(&tab.location) || !seen.insert((side, tab.location.clone())) {
                row.status = refused("exchange_file_invalid");
                rows.push(row);
                continue;
            }
            if on_share(&tab.location) {
                row.warning = Some("network_location".to_owned());
            }
            let open = self
                .tabs(tab.panel)
                .iter()
                .any(|open| open.path == tab.location);
            row.status = if open {
                RowStatus::Exists
            } else {
                RowStatus::New
            };
            choose(&mut row, only);
            rows.push(row);
        }
        if dry_run {
            return Ok(rows);
        }
        for panel in PanelId::BOTH {
            let picked: Vec<usize> = (0..rows.len())
                .filter(|&at| rows[at].selected && tabs[at].panel == panel)
                .collect();
            let added = picked
                .iter()
                .map(|&at| (tabs[at].location.clone(), tabs[at].locked))
                .collect();
            let result = self.add_imported_tabs(panel, added, 0, false).await;
            let failed = result.as_ref().err().map(EngineError::code);
            for at in picked {
                rows[at].status = match failed {
                    Some(code) => RowStatus::Failed {
                        code: code.to_owned(),
                    },
                    None => RowStatus::Created,
                };
            }
        }
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "exchange_places_tests.rs"]
mod tests;
