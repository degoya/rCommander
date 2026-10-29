//! Moving between folders without typing one: the folder history (`history.*`, Alt+Left,
//! Alt+Right, Alt+Down), swapping the panels (`panel.swap`, Ctrl+U and Ctrl+Shift+U) and showing
//! a panel's folder in the other one (`panel.transfer`, Ctrl+Left and Ctrl+Right).

use rc_vfs::VfsPath;

use super::{
    TabsError,
    requests::{HistoryJumpRequest, TransferDirRequest},
};
use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, commands::resolve_path,
};

impl Engine {
    /// `history.back` (`by` -1) and `history.forward` (1).
    pub(crate) async fn history_step(
        &self,
        panel: Option<PanelId>,
        by: isize,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (index, target) = {
            let state = self.state();
            let history = &state.tabs.set(id).active_tab().history;
            let (index, target) = history.step(by).ok_or(TabsError::HistoryEnd)?;
            (index, target.clone())
        };
        self.history_go(id, index, target).await
    }

    /// `history.jump`.
    pub(crate) async fn history_jump(
        &self,
        request: HistoryJumpRequest,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let target = {
            let state = self.state();
            let entries = state.tabs.set(id).active_tab().history.entries();
            entries
                .get(request.index)
                .cloned()
                .ok_or(TabsError::HistoryOutOfRange {
                    index: request.index,
                    len: entries.len(),
                })?
        };
        self.history_go(id, request.index, target).await
    }

    /// `history.list`: the active tab's folders, oldest first, and the one shown.
    pub(crate) fn history_list(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        let (entries, current) = self.history(id);
        Outcome::History {
            panel: id,
            entries,
            current,
        }
    }

    /// The folders of `panel`'s shown tab, oldest first, and the index of the one shown: what
    /// the window's history list draws.
    #[must_use]
    pub fn history(&self, panel: PanelId) -> (Vec<VfsPath>, usize) {
        let state = self.state();
        let history = &state.tabs.set(panel).active_tab().history;
        (history.entries().to_vec(), history.pos())
    }

    /// The active tab of `id` shows entry `index` of its history, `target`. The history's place
    /// moves first, so the visit `show_path` records is the folder it already stands on; it
    /// moves back when the folder cannot be shown. A locked tab opens the folder in a new tab.
    async fn history_go(
        &self,
        id: PanelId,
        index: usize,
        target: VfsPath,
    ) -> Result<Outcome, EngineError> {
        let previous = {
            let mut state = self.state();
            let tab = state.tabs.set_mut(id).active_tab_mut();
            if tab.locked {
                None
            } else {
                let previous = tab.history.pos();
                tab.history.go(index);
                Some(previous)
            }
        };
        let Some(previous) = previous else {
            return self.open_tab_at(id, target).await;
        };
        let shown = self.show_path(id, target).await;
        if shown.is_err() {
            let mut state = self.state();
            state.tabs.set_mut(id).active_tab_mut().history.go(previous);
        }
        shown
    }

    /// `panel.swap`: the panels exchange their folders, with cursor, selection, sort and mode
    /// (Ctrl+U), or their whole tab rows (Ctrl+Shift+U). Each panel keeps showing its files or
    /// its terminal; locked tabs take the other folder too, as the swap is asked for by name.
    pub(crate) fn swap_panels(&self, with_tabs: bool) -> Outcome {
        let snapshot = {
            let mut guard = self.state();
            let state = &mut *guard;
            state.tabs.sync(PanelId::Left, &state.left);
            state.tabs.sync(PanelId::Right, &state.right);
            let views = (state.left.view, state.right.view);
            std::mem::swap(&mut state.left, &mut state.right);
            (state.left.view, state.right.view) = views;
            if with_tabs {
                state.tabs.swap();
            } else {
                state.tabs.visited(PanelId::Left, &state.left.path);
                state.tabs.visited(PanelId::Right, &state.right.path);
            }
            let active = state.active;
            state.panel(active).snapshot(active, true)
        };
        for panel in PanelId::BOTH {
            self.publish(EngineEvent::Panel {
                panel,
                change: PanelChange::Navigated,
            });
        }
        Outcome::Panel(snapshot)
    }

    /// `panel.transfer`: panel `side` shows the folder under `panel`'s cursor, or `panel`'s own
    /// when the cursor is on a file, or the given path.
    pub(crate) async fn transfer_dir(
        &self,
        request: TransferDirRequest,
    ) -> Result<Outcome, EngineError> {
        let source = self.resolve(request.panel);
        let path = match request.path {
            Some(path) => {
                let current = self.state().panel(source).path.clone();
                resolve_path(&current, &path)
            }
            None => self.cursor_dir(source),
        };
        self.show_path(request.side, path).await
    }
}

#[cfg(test)]
#[path = "moves_tests.rs"]
mod tests;
