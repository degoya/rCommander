//! Total Commander's own commands and Start menu against rCommander's (RC-100-86): the plan a
//! preview shows, and with it the writes, through the same checked stores the window uses
//! (`Engine::store_user_command`, `Engine::store_start_item`).
//!
//! - An own command is the same as a stored one of the same name (case does not count): unchanged
//!   when its command, tooltip and symbol are the same too, else updated.
//! - The Start menu goes into the submenu [`DEFAULT_FOLDER`] appended at the top, Total
//!   Commander's submenus into submenus inside. A row is the same as one already there when it
//!   has the same kind and title in the same submenu (ignoring ASCII case; separators by their
//!   order): unchanged when its command is the same too, else updated. A second import therefore
//!   adds nothing twice. Submenus are made when something selected goes in.
//! - Which program runs is a person's choice: over MCP every row that would change something is
//!   skipped with `needs_gui`, as `usercmd.save` and `start_menu.save` are the window's.

use std::collections::HashMap;

use super::{
    total_commander::TcSkip,
    total_commander_apply::{chosen, path_key},
    total_commander_commands::{TcCommand, TcStartEntry},
    total_commander_report::{DEFAULT_FOLDER, TcCommandRow, TcRowStatus, TcStartRow},
};
use crate::{
    Engine, EngineError, StartItem, StartItemDraft, StartItemId, StartItemKind, UserCommandDraft,
    UserCommandError, read_guard,
};

/// A row skipped for `skip`.
fn skipped(skip: TcSkip) -> TcRowStatus {
    TcRowStatus::Skipped {
        reason: skip.code().to_owned(),
    }
}

/// Whether the planned `status` would change something an agent may not: then `needs_gui`.
fn held_from_mcp(status: &TcRowStatus) -> bool {
    matches!(status, TcRowStatus::New | TcRowStatus::Update) && read_guard::from_mcp()
}

/// The Start menu as the plan walks it: its rows, and those already matched.
struct Existing {
    items: Vec<StartItem>,
    claimed: Vec<StartItemId>,
}

impl Existing {
    /// The first unclaimed row of `kind` titled `title` in `parent`.
    fn take(
        &mut self,
        parent: Option<StartItemId>,
        title: &str,
        kind: StartItemKind,
    ) -> Option<StartItem> {
        let found = self.items.iter().find(|item| {
            item.parent == parent
                && item.kind == kind
                && item.title.eq_ignore_ascii_case(title)
                && !self.claimed.contains(&item.id)
        })?;
        self.claimed.push(found.id);
        Some(found.clone())
    }
}

impl Engine {
    /// The own commands `found` against the stored ones; written unless `dry_run`.
    pub(crate) async fn import_tc_commands(
        &self,
        found: &[TcCommand],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<TcCommandRow>, EngineError> {
        let existing = if found.is_empty() {
            Vec::new()
        } else {
            self.user_commands().await?
        };
        let mut rows = Vec::with_capacity(found.len());
        let mut plans: Vec<(Option<i64>, Option<&UserCommandDraft>)> = Vec::new();
        for command in found {
            let mut row = TcCommandRow {
                key: command.section.trim().to_ascii_lowercase(),
                name: command.section.trim().to_owned(),
                action: None,
                tooltip: String::new(),
                icon_path: String::new(),
                icon_index: 0,
                unknown_rows: command.unknown.clone(),
                selected: false,
                status: TcRowStatus::New,
            };
            let mut id = None;
            match &command.draft {
                Err(skip) => row.status = skipped(*skip),
                Ok(draft) => {
                    row.name.clone_from(&draft.name);
                    row.action = Some(draft.action.clone());
                    row.tooltip.clone_from(&draft.tooltip);
                    row.icon_path.clone_from(&draft.icon_path);
                    row.icon_index = draft.icon_index;
                    if let Some(there) = existing.iter().find(|there| there.name == draft.name) {
                        let same = there.action == draft.action
                            && there.tooltip == draft.tooltip
                            && there.icon_path == draft.icon_path
                            && there.icon_index == draft.icon_index;
                        row.status = if same {
                            TcRowStatus::Exists
                        } else {
                            TcRowStatus::Update
                        };
                        id = Some(there.id);
                    }
                    if held_from_mcp(&row.status) {
                        row.status = skipped(TcSkip::NeedsGui);
                    }
                    let importable = matches!(row.status, TcRowStatus::New | TcRowStatus::Update);
                    row.selected = importable && chosen(&row.key, only, true);
                }
            }
            plans.push((id, command.draft.as_ref().ok()));
            rows.push(row);
        }
        if dry_run {
            return Ok(rows);
        }
        for (row, (id, draft)) in rows.iter_mut().zip(plans) {
            let Some(draft) = draft else {
                continue;
            };
            if !row.selected || !matches!(row.status, TcRowStatus::New | TcRowStatus::Update) {
                continue;
            }
            self.store_user_command(id, draft).await?;
            row.status = match id {
                Some(_) => TcRowStatus::Updated,
                None => TcRowStatus::Created,
            };
        }
        Ok(rows)
    }

    /// The Start menu `found` against the stored one; written unless `dry_run`.
    pub(crate) async fn import_tc_start_menu(
        &self,
        found: &[TcStartEntry],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<TcStartRow>, EngineError> {
        let mut existing = Existing {
            items: if found.is_empty() {
                Vec::new()
            } else {
                self.start_menu().await?
            },
            claimed: Vec::new(),
        };
        // Each submenu path (lower case) to its row; `None` while it does not exist.
        let mut folders: HashMap<Vec<String>, Option<StartItemId>> = HashMap::new();
        folders.insert(
            Vec::new(),
            existing
                .take(None, DEFAULT_FOLDER, StartItemKind::Submenu)
                .map(|item| item.id),
        );
        let mut rows = Vec::with_capacity(found.len());
        let mut ids = Vec::with_capacity(found.len());
        for entry in found {
            let parent = folders.get(&path_key(&entry.folders)).copied().flatten();
            let mut row = TcStartRow {
                key: format!("start{}", entry.index),
                title: entry.title.clone(),
                folders: entry.folders.clone(),
                kind: entry.kind,
                action: entry.action.clone(),
                unknown_rows: entry.unknown.clone(),
                selected: false,
                status: TcRowStatus::New,
            };
            let mut id = None;
            if let Some(skip) = entry.skip {
                row.status = skipped(skip);
            } else {
                let there =
                    parent.and_then(|parent| existing.take(Some(parent), &entry.title, entry.kind));
                id = there.as_ref().map(|there| there.id);
                if let Some(there) = &there {
                    row.status = if there.action == entry.action {
                        TcRowStatus::Exists
                    } else {
                        TcRowStatus::Update
                    };
                }
                if entry.kind == StartItemKind::Submenu {
                    let mut path = entry.folders.clone();
                    path.push(entry.title.clone());
                    folders.insert(path_key(&path), id);
                }
                if held_from_mcp(&row.status) {
                    row.status = skipped(TcSkip::NeedsGui);
                }
                let importable = matches!(row.status, TcRowStatus::New | TcRowStatus::Update);
                // A submenu is made anyway for what goes into it; chosen by itself only by key.
                let default = entry.kind != StartItemKind::Submenu || there.is_none();
                row.selected = importable && chosen(&row.key, only, default);
            }
            rows.push(row);
            ids.push(id);
        }
        if dry_run {
            return Ok(rows);
        }
        for (row, there) in rows.iter_mut().zip(&ids) {
            if !row.selected || !matches!(row.status, TcRowStatus::New | TcRowStatus::Update) {
                continue;
            }
            if row.kind == StartItemKind::Submenu {
                let mut path = row.folders.clone();
                path.push(row.title.clone());
                self.start_folder(&mut folders, &path).await?;
                row.status = TcRowStatus::Created;
                continue;
            }
            let draft = StartItemDraft {
                kind: row.kind,
                title: row.title.clone(),
                action: row.action.clone(),
            };
            row.status = match (&row.status, *there) {
                (TcRowStatus::Update, Some(id)) => {
                    self.store_start_item(Some(id), None, &draft).await?;
                    TcRowStatus::Updated
                }
                _ => {
                    let parent = self.start_folder(&mut folders, &row.folders).await?;
                    self.store_start_item(None, Some(parent), &draft).await?;
                    TcRowStatus::Created
                }
            };
        }
        // A submenu made for what it holds although it was not chosen itself.
        for row in rows
            .iter_mut()
            .filter(|row| row.kind == StartItemKind::Submenu && row.status == TcRowStatus::New)
        {
            let mut path = row.folders.clone();
            path.push(row.title.clone());
            if folders.get(&path_key(&path)).copied().flatten().is_some() {
                row.status = TcRowStatus::Created;
            }
        }
        Ok(rows)
    }

    /// The submenu of the path `names` below [`DEFAULT_FOLDER`], made with the submenus above it
    /// where missing; [`DEFAULT_FOLDER`] itself goes after the last row at the top.
    async fn start_folder(
        &self,
        folders: &mut HashMap<Vec<String>, Option<StartItemId>>,
        names: &[String],
    ) -> Result<StartItemId, EngineError> {
        let mut parent = None;
        for depth in 0..=names.len() {
            let key = path_key(&names[..depth]);
            if let Some(Some(id)) = folders.get(&key) {
                parent = Some(*id);
                continue;
            }
            let title = match depth {
                0 => DEFAULT_FOLDER,
                _ => names[depth - 1].as_str(),
            };
            let draft = StartItemDraft {
                kind: StartItemKind::Submenu,
                title: title.to_owned(),
                action: None,
            };
            let made = self.store_start_item(None, parent, &draft).await?;
            folders.insert(key, Some(made));
            parent = Some(made);
        }
        parent.ok_or_else(|| {
            UserCommandError::StartInvalid {
                reason: "the import's submenu was not made".to_owned(),
            }
            .into()
        })
    }
}

#[cfg(test)]
#[path = "total_commander_commands_apply_tests.rs"]
mod tests;
