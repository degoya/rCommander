//! The directory menu and the tabs of `wincmd.ini` against the hotlist and the panels
//! (RC-050-09): the plan a preview shows, and with it the writes.
//!
//! - The menu goes into the hotlist folder [`DEFAULT_FOLDER`] at the top, its submenus into
//!   folders inside. An item is the same as one already there when it has the same name in the
//!   same folder (ignoring ASCII case): unchanged when its folders are the same too, else updated.
//!   A second import therefore adds nothing twice. Folders are made when something selected
//!   goes in.
//! - A tab is there already when the panel has a tab on its folder (adding only); a tab whose
//!   folder does not exist is `missing` and added only when `only` names it.

use std::{collections::HashMap, time::Duration};

use rc_db::{HotlistItem, HotlistPatch, NewHotlistItem};
use rc_vfs::VfsPath;

use super::{
    total_commander::{TcMenuItem, TcMenuKind, TcTab},
    total_commander_report::{DEFAULT_FOLDER, TcHotlistRow, TcRowStatus, TcTabRow, TcTabsMode},
};
use crate::{Engine, EngineError, EngineEvent, PanelId};

/// How long a tab's folder may take to answer whether it exists (a server share may not).
const EXISTS_TIMEOUT: Duration = Duration::from_secs(2);

/// Whether the row `key` is written: named in `only`, or with no `only` when `default`.
pub(crate) fn chosen(key: &str, only: Option<&[String]>, default: bool) -> bool {
    only.map_or(default, |keys| keys.iter().any(|named| named == key))
}

/// A folder path as a map key: its names in ASCII lower case.
fn path_key(names: &[String]) -> Vec<String> {
    names.iter().map(|name| name.to_ascii_lowercase()).collect()
}

/// `path` as a hotlist stores it.
fn location(path: &str) -> Result<String, EngineError> {
    serde_json::to_string(&VfsPath::local(path))
        .map_err(|_| crate::tabs::TabsError::HotlistBadPath.into())
}

/// Whether the stored JSON `stored` is the folder `path`.
fn same_place(stored: Option<&String>, path: Option<&str>) -> bool {
    let stored: Option<VfsPath> = stored.and_then(|json| serde_json::from_str(json).ok());
    stored == path.map(VfsPath::local)
}

/// The hotlist as the plan walks it: rows by parent, and those already matched.
struct Existing {
    items: Vec<HotlistItem>,
    claimed: Vec<i64>,
}

impl Existing {
    /// The first unclaimed row named `name` in `parent` that is a folder or not.
    fn take(&mut self, parent: Option<i64>, name: &str, folder: bool) -> Option<HotlistItem> {
        let found = self.items.iter().find(|item| {
            item.parent_id == parent
                && item.is_folder() == folder
                && item.name.eq_ignore_ascii_case(name)
                && !self.claimed.contains(&item.id)
        })?;
        self.claimed.push(found.id);
        Some(found.clone())
    }
}

impl Engine {
    /// The menu `items` against the hotlist; written unless `dry_run`.
    pub(crate) async fn import_tc_hotlist(
        &self,
        items: &[TcMenuItem],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<TcHotlistRow>, EngineError> {
        let db = self.db()?;
        let mut existing = Existing {
            items: db.hotlist().await?,
            claimed: Vec::new(),
        };
        // Each folder path (lower case) to its row; `None` while it does not exist.
        let mut folders: HashMap<Vec<String>, Option<i64>> = HashMap::new();
        folders.insert(
            Vec::new(),
            existing
                .take(None, DEFAULT_FOLDER, true)
                .map(|item| item.id),
        );
        let mut rows = Vec::with_capacity(items.len());
        let mut found = Vec::with_capacity(items.len());
        for item in items {
            let key = format!("menu{}", item.index);
            let parent = folders.get(&path_key(&item.folders)).copied().flatten();
            let mut row = TcHotlistRow {
                key,
                name: item.name.clone(),
                folders: item.folders.clone(),
                folder: matches!(item.kind, TcMenuKind::Folder),
                path: None,
                target: None,
                selected: false,
                status: TcRowStatus::New,
            };
            let mut id = None;
            match &item.kind {
                TcMenuKind::Skipped(skip) => {
                    row.status = TcRowStatus::Skipped {
                        reason: skip.code().to_owned(),
                    };
                }
                TcMenuKind::Folder => {
                    let mut path = item.folders.clone();
                    path.push(item.name.clone());
                    let there =
                        parent.and_then(|parent| existing.take(Some(parent), &item.name, true));
                    if there.is_some() {
                        row.status = TcRowStatus::Exists;
                    }
                    id = there.as_ref().map(|there| there.id);
                    folders.insert(path_key(&path), id);
                    row.selected = chosen(&row.key, only, there.is_none());
                }
                TcMenuKind::Entry { path, target } => {
                    row.path = Some(path.clone());
                    row.target.clone_from(target);
                    if let Some(there) =
                        parent.and_then(|parent| existing.take(Some(parent), &item.name, false))
                    {
                        let same = same_place(there.location.as_ref(), Some(path))
                            && same_place(there.target.as_ref(), target.as_deref());
                        row.status = if same {
                            TcRowStatus::Exists
                        } else {
                            TcRowStatus::Update
                        };
                        id = Some(there.id);
                    }
                    let importable = row.status != TcRowStatus::Exists;
                    row.selected = importable && chosen(&row.key, only, true);
                }
            }
            rows.push(row);
            found.push(id);
        }
        if dry_run {
            return Ok(rows);
        }
        let mut changed = false;
        for (row, there) in rows.iter_mut().zip(&found) {
            if !row.selected || !matches!(row.status, TcRowStatus::New | TcRowStatus::Update) {
                continue;
            }
            let done = if row.folder {
                let mut path = row.folders.clone();
                path.push(row.name.clone());
                self.hotlist_folder(&mut folders, &path).await?;
                TcRowStatus::Created
            } else if let (TcRowStatus::Update, Some(id)) = (&row.status, *there) {
                let patch = HotlistPatch {
                    name: None,
                    location: Some(location(row.path.as_deref().unwrap_or_default())?),
                    target: Some(row.target.as_deref().map(location).transpose()?),
                };
                db.update_hotlist(id, &patch).await?;
                TcRowStatus::Updated
            } else {
                let parent = self.hotlist_folder(&mut folders, &row.folders).await?;
                let path = location(row.path.as_deref().unwrap_or_default())?;
                let target = row.target.as_deref().map(location).transpose()?;
                db.add_hotlist(&NewHotlistItem {
                    parent_id: Some(parent),
                    name: &row.name,
                    location: Some(&path),
                    target: target.as_deref(),
                })
                .await?;
                TcRowStatus::Created
            };
            row.status = done;
            changed = true;
        }
        // A submenu made for what it holds although it was not chosen itself.
        for row in rows
            .iter_mut()
            .filter(|row| row.folder && row.status == TcRowStatus::New)
        {
            let mut path = row.folders.clone();
            path.push(row.name.clone());
            if folders.get(&path_key(&path)).copied().flatten().is_some() {
                row.status = TcRowStatus::Created;
            }
        }
        if changed {
            self.publish(EngineEvent::HotlistChanged);
        }
        Ok(rows)
    }

    /// The hotlist folder of the submenu path `names` below [`DEFAULT_FOLDER`], made with the
    /// folders above it where missing.
    async fn hotlist_folder(
        &self,
        folders: &mut HashMap<Vec<String>, Option<i64>>,
        names: &[String],
    ) -> Result<i64, EngineError> {
        let db = self.db()?;
        let mut parent = None;
        for depth in 0..=names.len() {
            let key = path_key(&names[..depth]);
            if let Some(Some(id)) = folders.get(&key) {
                parent = Some(*id);
                continue;
            }
            let name = match depth {
                0 => DEFAULT_FOLDER,
                _ => names[depth - 1].as_str(),
            };
            let made = db
                .add_hotlist(&NewHotlistItem {
                    parent_id: parent,
                    name,
                    location: None,
                    target: None,
                })
                .await?;
            folders.insert(key, Some(made.id));
            parent = Some(made.id);
        }
        parent.ok_or_else(|| crate::tabs::TabsError::HotlistBadPath.into())
    }

    /// The tabs against the panels; added unless `dry_run`.
    pub(crate) async fn import_tc_tabs(
        &self,
        tabs: &[TcTab],
        only: Option<&[String]>,
        mode: TcTabsMode,
        dry_run: bool,
    ) -> Result<Vec<TcTabRow>, EngineError> {
        let mut rows = Vec::with_capacity(tabs.len());
        for tab in tabs {
            let side = match tab.panel {
                PanelId::Left => "left",
                PanelId::Right => "right",
            };
            let key = format!("{side}{}", tab.position);
            let exists = match &tab.path {
                Ok(path) => folder_exists(path).await,
                Err(_) => false,
            };
            let status = match &tab.path {
                Err(skip) => TcRowStatus::Skipped {
                    reason: skip.code().to_owned(),
                },
                Ok(_) if !exists => TcRowStatus::Missing,
                Ok(path)
                    if mode == TcTabsMode::Append
                        && self
                            .tabs(tab.panel)
                            .iter()
                            .any(|open| open.path == VfsPath::local(path)) =>
                {
                    TcRowStatus::Exists
                }
                Ok(_) => TcRowStatus::New,
            };
            let selected = match status {
                TcRowStatus::New => chosen(&key, only, true),
                TcRowStatus::Missing => chosen(&key, only, false),
                _ => false,
            };
            rows.push(TcTabRow {
                key,
                panel: tab.panel,
                path: tab.path.clone().ok(),
                caption: tab.caption.clone(),
                locked: tab.locked,
                active: tab.active,
                selected,
                status,
            });
        }
        if dry_run {
            return Ok(rows);
        }
        for panel in PanelId::BOTH {
            let picked: Vec<usize> = (0..rows.len())
                .filter(|&at| rows[at].panel == panel && rows[at].selected)
                .collect();
            let active = picked.iter().position(|&at| rows[at].active).unwrap_or(0);
            let tabs = picked
                .iter()
                .filter_map(|&at| {
                    let path = rows[at].path.as_deref()?;
                    Some((VfsPath::local(path), rows[at].locked))
                })
                .collect();
            self.add_imported_tabs(panel, tabs, active, mode == TcTabsMode::Replace)
                .await?;
            for at in picked {
                rows[at].status = TcRowStatus::Created;
            }
        }
        Ok(rows)
    }
}

/// Whether `path` is a folder now, asked for at most [`EXISTS_TIMEOUT`].
async fn folder_exists(path: &str) -> bool {
    matches!(
        tokio::time::timeout(EXISTS_TIMEOUT, tokio::fs::metadata(path)).await,
        Ok(Ok(metadata)) if metadata.is_dir()
    )
}

#[cfg(test)]
#[path = "total_commander_apply_tests.rs"]
mod tests;
