//! The size column's counted folders (RC-020-06): Space on a folder (`panel.select_and_size`)
//! and Alt+Shift+Enter for all of them (`panel.count_dir_content`), as in Total Commander.
//!
//! A count runs in the background, one folder after the other, and answers at once. The results
//! land in the panel's [`crate::PanelState::dir_sizes`] in batches (RC-110-63): what was counted
//! within [`SIZES_EVERY`] goes in together, with one copy of the map and one listing change, so
//! 10 000 small folders cost a copy and an event per tenth of a second, not one per folder (the
//! window holds a copy of the map, so each change copies it, and a burst of events overflows a
//! window's event buffer). A folder that takes longer is shown as soon as it is counted.
//! `panel.cancel_dir_sizes` stops the run, and what was counted but not shown yet goes with what
//! is still counting; a new listing of the panel drops the results, and a batch with a folder the
//! panel no longer lists ends the run.

use std::{sync::Arc, time::Duration};

use rc_vfs::{EntryKind, VfsPath};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::settings::SPACE_MOVES_DOWN_KEY;
use crate::{DirSize, Engine, EngineEvent, Outcome, PanelChange, PanelId};

/// How long a counted folder waits at most to be shown with the ones counted after it.
pub(crate) const SIZES_EVERY: Duration = Duration::from_millis(100);

/// Folders counted and not shown yet: each name with its bytes, `None` when it could not be
/// counted.
struct Batch {
    counted: Vec<(String, Option<u64>)>,
    shown: Instant,
}

impl Batch {
    fn new() -> Self {
        Self {
            counted: Vec::new(),
            shown: Instant::now(),
        }
    }

    /// When what waits is to be shown.
    fn due(&self) -> Instant {
        self.shown + SIZES_EVERY
    }
}

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
            let sizes = &mut state.panel_mut(id).dir_sizes;
            // Nothing counting: the map stays the one every copy of the state shares.
            if sizes.values().any(|size| *size == DirSize::Counting) {
                Arc::make_mut(sizes).retain(|_, size| *size != DirSize::Counting);
            }
        }
        self.announce_sizes(id);
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// Marks `names` as being counted and counts them, one after the other, off this call.
    fn count_folders(&self, id: PanelId, names: Vec<String>) {
        let names: Vec<String> = {
            let mut state = self.state();
            let sizes = &mut state.panel_mut(id).dir_sizes;
            let fresh: Vec<String> = names
                .into_iter()
                .filter(|name| sizes.get(name) != Some(&DirSize::Counting))
                .collect();
            if !fresh.is_empty() {
                let sizes = Arc::make_mut(sizes);
                for name in &fresh {
                    sizes.insert(name.clone(), DirSize::Counting);
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
        // What the ignore list hides is not counted (RC-100-92).
        let skip = crate::ignore_list::skip_on(dir.backend.clone());
        tokio::spawn(async move {
            let mut batch = Batch::new();
            for name in names {
                let path = dir.path.join(&name);
                let counting = async {
                    match &skip {
                        Some(skip) => rc_ops::dir_size_without(&*vfs, &path, skip).await,
                        None => rc_ops::dir_size(&*vfs, &path).await,
                    }
                };
                tokio::pin!(counting);
                let counted = loop {
                    let (due, waiting) = (batch.due(), !batch.counted.is_empty());
                    tokio::select! {
                        () = token.cancelled() => return,
                        counted = &mut counting => break counted,
                        // A long count does not hold back what was counted before it.
                        () = tokio::time::sleep_until(due), if waiting => {
                            if !engine.show_sizes(id, &dir, &mut batch) {
                                return;
                            }
                        }
                    }
                };
                batch
                    .counted
                    .push((name, counted.ok().map(|usage| usage.bytes)));
                if Instant::now() >= batch.due() && !engine.show_sizes(id, &dir, &mut batch) {
                    return;
                }
            }
            engine.show_sizes(id, &dir, &mut batch);
        });
    }

    /// Puts what `batch` counted into the panel's sizes, with one copy of the map and one
    /// listing change. `false` when the panel no longer shows the listing the run counts for -
    /// it moved on, or listed its folder again - and the rest is not its.
    fn show_sizes(&self, id: PanelId, dir: &VfsPath, batch: &mut Batch) -> bool {
        let counted = std::mem::take(&mut batch.counted);
        batch.shown = Instant::now();
        if counted.is_empty() {
            return true;
        }
        let (changed, listed) = {
            let mut state = self.state();
            let panel = state.panel_mut(id);
            if panel.path != *dir {
                return false;
            }
            let kept = counted
                .iter()
                .filter(|(name, _)| panel.dir_sizes.contains_key(name))
                .count();
            if kept > 0 {
                let sizes = Arc::make_mut(&mut panel.dir_sizes);
                for (name, bytes) in &counted {
                    match bytes {
                        Some(bytes) => {
                            if let Some(size) = sizes.get_mut(name) {
                                *size = DirSize::Bytes(*bytes);
                            }
                        }
                        None => {
                            sizes.remove(name);
                        }
                    }
                }
            }
            (kept > 0, kept == counted.len())
        };
        if changed {
            self.announce_sizes(id);
        }
        listed
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
