//! The keyboard's commands: they act on the entry under the cursor or move the cursor by a step,
//! so a keymap entry needs no name and no index.

use crate::{
    CursorStep, Engine, EngineError, EngineEvent, ListMode, Outcome, PanelChange, PanelId,
};

impl Engine {
    /// `panel.move_cursor`: a step from where the cursor stands. A page is one row less than
    /// the window last reported ([`Engine::set_page_rows`]), so one row stays in view; a column
    /// of the brief view is all of them.
    pub(crate) fn move_cursor(&self, panel: Option<PanelId>, step: CursorStep) -> Outcome {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let (page, column) = state.paging(id);
            let panel = state.panel_mut(id);
            panel.cursor = stepped(panel.cursor, panel.entries.len(), page, column, step);
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Cursor,
        });
        Outcome::Panel(snapshot)
    }

    /// `panel.open_entry`: a directory (or a link to one) under the cursor is shown in the
    /// panel, and so is an archive, whatever its name (RC-030-02). Any other file, or no entry at
    /// all, leaves the panel as it is.
    pub(crate) async fn open_entry(&self, panel: Option<PanelId>) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let cursor = {
            let state = self.state();
            let panel = state.panel(id);
            panel
                .cursor_entry()
                .map(|entry| (entry.clone(), panel.path.clone()))
        };
        let cursor = match cursor {
            Some((entry, dir)) => {
                let is_dir = self.leads_to_dir(&dir.join(&entry.name), &entry).await?;
                Some((is_dir, entry.name, dir))
            }
            None => None,
        };
        match cursor {
            Some((true, name, _)) => self.navigate(Some(id), &name).await,
            Some((false, name, dir)) => match self.archive_root(&dir.join(&name)).await {
                Ok(Some(root)) => self.show_path(id, root).await,
                Ok(None) => Ok(Outcome::Panel(self.snapshot(Some(id)))),
                Err(error) => {
                    // Opened as any file then; why it is no archive goes into the log.
                    tracing::debug!(
                        code = error.code(),
                        "the file under the cursor did not open as an archive"
                    );
                    Ok(Outcome::Panel(self.snapshot(Some(id))))
                }
            },
            None => Ok(Outcome::Panel(self.snapshot(Some(id)))),
        }
    }

    /// `panel.toggle_cursor_selection`: the entry under the cursor flips its selection and the
    /// cursor moves one down, as Insert does in Total Commander.
    pub(crate) fn toggle_cursor_selection(&self, panel: Option<PanelId>) -> Outcome {
        self.flip_cursor_selection(panel, true)
    }

    /// The entry under the cursor flips its selection; the cursor moves one down when `down`.
    pub(crate) fn flip_cursor_selection(&self, panel: Option<PanelId>, down: bool) -> Outcome {
        let id = self.resolve(panel);
        let (snapshot, flipped) = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            let flipped = match panel.cursor_entry().map(|entry| entry.name.clone()) {
                Some(name) => {
                    if !panel.selection.remove(&name) {
                        panel.selection.insert(name);
                    }
                    true
                }
                None => false,
            };
            if down {
                panel.cursor = stepped(panel.cursor, panel.entries.len(), 1, 0, CursorStep::Down);
            }
            (panel.snapshot(id, active), flipped)
        };
        if flipped {
            self.publish(EngineEvent::Panel {
                panel: id,
                change: PanelChange::Selection,
            });
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Cursor,
        });
        Outcome::Panel(snapshot)
    }
}

/// `cursor` moved by `step` among `len` entries, a page being `page` entries and a column of the
/// brief view `column` (0 in the full view); 0 when there are none.
pub(crate) fn stepped(
    cursor: usize,
    len: usize,
    page: usize,
    column: usize,
    step: CursorStep,
) -> usize {
    let Some(last) = len.checked_sub(1) else {
        return 0;
    };
    reach(cursor, len, page, column, step).clamp(0, last as isize) as usize
}

/// Where `step` leads from `cursor` before it is held to the entries: before the first one or
/// past the last when the step is cut short.
pub(crate) fn reach(
    cursor: usize,
    len: usize,
    page: usize,
    column: usize,
    step: CursorStep,
) -> isize {
    let (cursor, page, column) = (cursor as isize, page as isize, column as isize);
    match step {
        CursorStep::Up => cursor - 1,
        CursorStep::Down => cursor + 1,
        CursorStep::PageUp => cursor - page,
        CursorStep::PageDown => cursor + page,
        CursorStep::First => 0,
        CursorStep::Last => len as isize - 1,
        CursorStep::Left => cursor - column,
        CursorStep::Right => cursor + column,
    }
}

impl crate::State {
    /// A page and a column of panel `id`, in entries: one row less than fits, so one row stays
    /// in view, and every row that fits in the brief view, none in the full view.
    pub(crate) fn paging(&self, id: PanelId) -> (usize, usize) {
        let rows = self.page_rows[id.index()];
        let column = match self.panel(id).mode {
            ListMode::Brief => rows,
            ListMode::Full => 0,
        };
        (rows.saturating_sub(1).max(1), column)
    }
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod tests;
