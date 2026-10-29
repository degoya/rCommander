//! The size column's counted folders (RC-020-06): Space on a folder (`panel.select_and_size`)
//! and Alt+Shift+Enter for all of them (`panel.count_dir_content`), as in Total Commander.
//!
//! A count runs in the background, one folder after the other, and answers at once; each
//! result lands in the panel's [`crate::PanelState::dir_sizes`] and is announced as a listing
//! change. `panel.cancel_dir_sizes` stops it; a new listing of the panel drops the results, and
//! the first result for a folder the panel no longer lists ends the run.

use rc_vfs::EntryKind;
use tokio_util::sync::CancellationToken;

use crate::settings::SPACE_MOVES_DOWN_KEY;
use crate::{DirSize, Engine, EngineEvent, Outcome, PanelChange, PanelId};

impl Engine {
    /// `panel.select_and_size`: the cursor entry's selection flips and the cursor moves down,
    /// as Insert does, or stays with `selection.space_moves_down` off; a folder under the cursor
    /// is counted too.
    pub(crate) async fn select_and_size(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        let folder = self
            .state()
            .panel(id)
            .cursor_entry()
            .filter(|entry| entry.kind == EntryKind::Dir)
            .map(|entry| entry.name.clone());
        let down = self.setting_on(SPACE_MOVES_DOWN_KEY).await;
        let outcome = self.flip_cursor_selection(Some(id), down);
        if let Some(name) = folder {
            self.count_folders(id, vec![name]);
        }
        outcome
    }

    /// `panel.count_dir_content`: every folder of the panel not counted yet.
    pub(crate) fn count_dir_content(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        let names = {
            let state = self.state();
            let panel = state.panel(id);
            panel
                .entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Dir)
                .filter(|entry| !panel.dir_sizes.contains_key(&entry.name))
                .map(|entry| entry.name.clone())
                .collect()
        };
        self.count_folders(id, names);
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// `panel.cancel_dir_sizes`: the running counts stop; what was counted stays.
    pub(crate) fn cancel_dir_sizes(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        {
            let mut state = self.state();
            if let Some(token) = state.counting[id.index()].take() {
                token.cancel();
            }
            state
                .panel_mut(id)
                .dir_sizes
                .retain(|_, size| *size != DirSize::Counting);
        }
        self.announce_sizes(id);
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// Marks `names` as being counted and counts them, one after the other, off this call.
    fn count_folders(&self, id: PanelId, names: Vec<String>) {
        let names: Vec<String> = {
            let mut state = self.state();
            let sizes = &mut state.panel_mut(id).dir_sizes;
            let mut fresh = Vec::with_capacity(names.len());
            for name in names {
                if sizes.get(&name) != Some(&DirSize::Counting) {
                    sizes.insert(name.clone(), DirSize::Counting);
                    fresh.push(name);
                }
            }
            fresh
        };
        if names.is_empty() {
            return;
        }
        let (dir, token) = {
            let mut state = self.state();
            let token = state.counting[id.index()]
                .get_or_insert_with(CancellationToken::new)
                .clone();
            (state.panel(id).path.clone(), token)
        };
        self.announce_sizes(id);
        let Ok(vfs) = self.vfs(&dir) else {
            return;
        };
        let engine = self.clone();
        tokio::spawn(async move {
            for name in names {
                let path = dir.path.join(&name);
                let counted = tokio::select! {
                    () = token.cancelled() => return,
                    counted = rc_ops::dir_size(&*vfs, &path) => counted,
                };
                {
                    let mut state = engine.state();
                    let panel = state.panel_mut(id);
                    // The panel moved on or listed its folder again: the rest is not its.
                    if panel.path != dir || !panel.dir_sizes.contains_key(&name) {
                        return;
                    }
                    match counted {
                        Ok(usage) => panel.dir_sizes.insert(name, DirSize::Bytes(usage.bytes)),
                        Err(_) => panel.dir_sizes.remove(&name),
                    };
                }
                engine.announce_sizes(id);
            }
        });
    }

    fn announce_sizes(&self, id: PanelId) {
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
    }
}

#[cfg(test)]
#[path = "dir_count_tests.rs"]
mod tests;
