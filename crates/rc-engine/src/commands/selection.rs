//! The selection commands of Total Commander: Ctrl+A or Ctrl+Num+ selects every entry, Ctrl+Num-
//! none, Num* turns the selection around (directories count like files); Num+ and Num- select and
//! deselect by a mask ("Select group", "Unselect group"), Alt+Num+ and Alt+Num- by the cursor entry's extension, Num / brings back
//! the selection before the last file operation, and Shift with a cursor key selects a range.
//! With `selection.include_folders` on, a mask without a directory part takes folders too
//! (RC-020-08); Alt+Num+ and Alt+Num- stay with files.

use rc_vfs::Entry;

use crate::{
    CursorStep, Engine, EngineError, EngineEvent, FlatSource, Outcome, PanelChange, PanelId,
    PanelState, extension,
    mask::Mask,
    search::{DuplicateSelection, Listed, apply_selection},
    settings::SELECT_FOLDERS_KEY,
    state::Selection,
};

impl Engine {
    /// `panel.select_all`.
    pub(crate) fn select_all(&self, panel: Option<PanelId>) -> Outcome {
        self.change_selection(panel, |state| {
            state.selection = state
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect();
        })
    }

    /// `panel.deselect_all`.
    pub(crate) fn deselect_all(&self, panel: Option<PanelId>) -> Outcome {
        self.change_selection(panel, |state| state.selection.clear())
    }

    /// `panel.invert_selection`.
    pub(crate) fn invert_selection(&self, panel: Option<PanelId>) -> Outcome {
        self.change_selection(panel, |state| {
            state.selection = state
                .entries
                .iter()
                .filter(|entry| !state.selection.contains(&entry.name))
                .map(|entry| entry.name.clone())
                .collect();
        })
    }

    /// `panel.select_by_mask` and `panel.deselect_by_mask`: the entries `mask` matches (`*.*`
    /// when `None`) are selected, or deselected; folders by the same names too when
    /// `selection.include_folders` is on ([`Mask::with_folders`]).
    pub(crate) async fn select_by_mask(
        &self,
        panel: Option<PanelId>,
        mask: Option<&str>,
        select: bool,
    ) -> Outcome {
        let mut mask = Mask::parse(mask.unwrap_or("*.*"));
        if self.setting_on(SELECT_FOLDERS_KEY).await {
            mask = mask.with_folders();
        }
        self.change_selection(panel, |state| {
            let PanelState {
                entries, selection, ..
            } = state;
            let matching = entries
                .iter()
                .filter(|entry| mask.matches(&entry.name, entry.is_dir_like()));
            mark(selection, matching, select);
        })
    }

    /// `panel.select_extension`: every file with the extension of the file under the cursor,
    /// ignoring case; a directory under the cursor changes nothing.
    pub(crate) fn select_extension(&self, panel: Option<PanelId>, select: bool) -> Outcome {
        self.change_selection(panel, |state| {
            let Some(wanted) = state
                .cursor_entry()
                .filter(|entry| !entry.is_dir_like())
                .map(|entry| extension(entry).to_lowercase())
            else {
                return;
            };
            let PanelState {
                entries, selection, ..
            } = state;
            let same = entries
                .iter()
                .filter(|entry| !entry.is_dir_like() && extension(entry).to_lowercase() == wanted);
            mark(selection, same, select);
        })
    }

    /// `panel.select_duplicates` (Num+ over a duplicate result, RC-100-98): the selection after
    /// `spec` over the panel's entries in list order, each with its group.
    ///
    /// # Errors
    ///
    /// The panel shows no duplicate result.
    pub(crate) fn select_duplicates(
        &self,
        panel: Option<PanelId>,
        spec: &DuplicateSelection,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let duplicates = self
            .state()
            .panel(id)
            .flat
            .as_ref()
            .is_some_and(|flat| flat.source == FlatSource::Duplicates);
        if !duplicates {
            return Err(EngineError::InvalidSearch {
                reason: "the panel shows no duplicate result".to_owned(),
            });
        }
        Ok(self.change_selection(Some(id), |state| {
            let PanelState {
                path,
                entries,
                selection,
                flat,
                ..
            } = state;
            let Some(flat) = flat else {
                return;
            };
            let groups = flat.plan.groups_by_name(&path.path);
            let files: Vec<Listed<'_>> = entries
                .iter()
                .filter_map(|entry| {
                    let group = *groups.get(&entry.name)?;
                    Some(Listed {
                        name: &entry.name,
                        group,
                        modified: entry.modified,
                        size: entry.size,
                    })
                })
                .collect();
            apply_selection(spec, &files, selection);
        }))
    }

    /// `panel.restore_selection`: the remembered names that are still entries; with nothing
    /// remembered the selection stays.
    pub(crate) fn restore_selection(&self, panel: Option<PanelId>) -> Outcome {
        self.change_selection(panel, |state| {
            if state.remembered.is_empty() {
                return;
            }
            state.selection = state
                .entries
                .iter()
                .filter(|entry| state.remembered.contains(&entry.name))
                .map(|entry| entry.name.clone())
                .collect();
        })
    }

    /// Remembers `panel`'s selection for Num / when a file operation takes it, which it does
    /// when it names no sources of its own.
    pub(crate) fn remember_selection(&self, panel: PanelId, named: &[String]) {
        if named.is_empty() {
            let mut state = self.state();
            let panel = state.panel_mut(panel);
            if !panel.selection.is_empty() {
                panel.remembered = panel.selection.clone();
            }
        }
    }

    /// `panel.select_range`: the cursor moves a step, and the entries it leaves behind get the
    /// opposite of the selection the cursor entry had - the target too when the step reaches
    /// the first or last entry, so Shift+End selects to the end.
    pub(crate) fn select_range(&self, panel: Option<PanelId>, step: CursorStep) -> Outcome {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let paging = state.paging(id);
            let panel = state.panel_mut(id);
            let (from, len) = (panel.cursor, panel.entries.len());
            if let Some(entry) = panel.cursor_entry() {
                let select = !panel.selection.contains(&entry.name);
                let to = paging.stepped(from, len, step);
                let cut_short = paging.reach(from, len, step) != to as isize;
                let inclusive = cut_short || matches!(step, CursorStep::First | CursorStep::Last);
                let range = match (from <= to, inclusive) {
                    (true, true) => from..to + 1,
                    (true, false) => from..to,
                    (false, true) => to..from + 1,
                    (false, false) => to + 1..from + 1,
                };
                let PanelState {
                    entries, selection, ..
                } = &mut *panel;
                mark(selection, entries[range].iter(), select);
                panel.cursor = to;
            }
            panel.snapshot(id, active)
        };
        for change in [PanelChange::Selection, PanelChange::Cursor] {
            self.publish(EngineEvent::Panel { panel: id, change });
        }
        Outcome::Panel(snapshot)
    }

    fn change_selection(
        &self,
        panel: Option<PanelId>,
        change: impl FnOnce(&mut PanelState),
    ) -> Outcome {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            change(panel);
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Selection,
        });
        Outcome::Panel(snapshot)
    }
}

/// Selects `entries`, or deselects them.
fn mark<'a>(selection: &mut Selection, entries: impl Iterator<Item = &'a Entry>, select: bool) {
    for entry in entries {
        if select {
            selection.insert(entry.name.clone());
        } else {
            selection.remove(&entry.name);
        }
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
