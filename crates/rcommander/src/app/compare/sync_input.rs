//! The synchronize-dirs dialog's messages and keys: the form, the comparison as a task, the
//! list, and the confirmation that hands the plan to the queue.

use std::sync::Arc;

use iced::Task;
use rc_engine::Chord;

use super::{CompareMsg, Flag, Phase, SyncDialog, sync_view, wrap};
use crate::app::{App, Effect};

impl App {
    pub(super) fn on_sync(&mut self, message: CompareMsg) -> Vec<Effect> {
        if let CompareMsg::SyncStarted(result) = message {
            // Also once the dialog is closed: the jobs run and are followed.
            return self.sync_started(result);
        }
        let Some(dialog) = self.compare.sync.as_mut() else {
            return Vec::new();
        };
        match message {
            CompareMsg::SyncMode(mode) => Self::set_mode(dialog, mode),
            CompareMsg::SyncFlag(flag, on) => set_flag(dialog, flag, on),
            CompareMsg::SyncTolerance(text) => {
                dialog.form.tolerance = text;
                dialog.invalid = false;
            }
            CompareMsg::SyncMasks(text) => dialog.form.masks = text,
            CompareMsg::SyncCompare => return self.sync_compare(),
            CompareMsg::SyncStop => dialog.stop(),
            CompareMsg::SyncPlanned(generation, result) => {
                if dialog.generation == generation {
                    dialog.planned(result);
                }
            }
            CompareMsg::SyncFilter(filter) => dialog.set_filter(filter),
            CompareMsg::SyncClick(at) => dialog.point_at(at),
            CompareMsg::SyncCycle(at) => {
                dialog.point_at(at);
                dialog.cycle(at);
            }
            CompareMsg::SyncOpen(at) => {
                dialog.point_at(at);
                return self.compare_row(at);
            }
            CompareMsg::SyncScroll(rows) => dialog.scroll(rows),
            CompareMsg::SyncViewport(size) => dialog.set_page(sync_view::rows_for(size.height)),
            CompareMsg::SyncAsk => {
                if dialog.phase != Phase::Comparing {
                    dialog.ask();
                }
            }
            CompareMsg::SyncRun => return self.sync_run(),
            CompareMsg::SyncCancelConfirm => dialog.confirm = None,
            CompareMsg::SyncClose => self.close_sync(),
            _ => {}
        }
        Vec::new()
    }

    /// A key while the dialog is open. With the confirmation open Enter runs and Escape goes
    /// back; otherwise Escape stops a comparison or closes, Enter compares, the cursor keys move
    /// in the list, Space changes the row's action and F3 compares its files. A key a text field
    /// took (`captured`) only counts for Escape.
    pub(super) fn sync_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        let Some(dialog) = self.compare.sync.as_mut() else {
            return Vec::new();
        };
        // Tab and Shift+Tab walk the focus ring - the confirmation's while it is open, as it is
        // the card on top (RC-080-16).
        if !captured && let Some(effects) = crate::app::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        let key = chord.key.as_str();
        if dialog.confirm.is_some() {
            match key {
                "Escape" => dialog.confirm = None,
                "Enter" if !captured => return self.sync_run(),
                _ => {}
            }
            return Vec::new();
        }
        let page = dialog.rows.page() as isize;
        match key {
            "Escape" if dialog.phase == Phase::Comparing => dialog.stop(),
            "Escape" => self.close_sync(),
            _ if captured => {}
            "Enter" => return self.sync_compare(),
            "ArrowUp" => dialog.step(-1),
            "ArrowDown" => dialog.step(1),
            "PageUp" => dialog.step(-page),
            "PageDown" => dialog.step(page),
            "Home" => dialog.step(isize::MIN),
            "End" => dialog.step(isize::MAX),
            "Space" => {
                let at = dialog.rows.cursor();
                dialog.cycle(at);
            }
            "F3" => {
                let at = dialog.rows.cursor();
                return self.compare_row(at);
            }
            _ => {}
        }
        Vec::new()
    }

    /// Compare: the plan of the two folders, in the background.
    fn sync_compare(&mut self) -> Vec<Effect> {
        let generation = self.compare.next_generation();
        let Some(dialog) = self.compare.sync.as_mut() else {
            return Vec::new();
        };
        let Some(options) = dialog.start(generation) else {
            return Vec::new();
        };
        let (engine, left, right) = (
            self.engine.clone(),
            dialog.left.clone(),
            dialog.right.clone(),
        );
        let cancel = dialog.cancel.clone();
        let plan = async move {
            engine
                .plan_sync(&left, &right, &options, &cancel)
                .await
                .map(Arc::new)
                .map_err(|error| error.code())
        };
        vec![Effect::Task(Task::perform(plan, move |result| {
            wrap(CompareMsg::SyncPlanned(generation, result))
        }))]
    }

    /// The confirmation's OK: the plan's jobs handed to the queue.
    fn sync_run(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.compare.sync.as_mut() else {
            return Vec::new();
        };
        let Some((plan, run)) = dialog.confirmed() else {
            return Vec::new();
        };
        dialog.confirm = None;
        dialog.phase = Phase::Starting;
        let engine = self.engine.clone();
        let start = async move {
            engine
                .sync_jobs(&plan, &run)
                .await
                .map_err(|error| error.code())
        };
        vec![Effect::Task(Task::perform(start, |result| {
            wrap(CompareMsg::SyncStarted(result))
        }))]
    }

    pub(in crate::app) fn close_sync(&mut self) {
        if let Some(dialog) = self.compare.sync.take() {
            dialog.stop();
        }
    }
}

fn set_flag(dialog: &mut SyncDialog, flag: Flag, on: bool) {
    let form = &mut dialog.form;
    let target = match flag {
        Flag::Subdirs => &mut form.subdirs,
        Flag::ByContent => &mut form.by_content,
        Flag::IgnoreDate => &mut form.ignore_date,
        Flag::IgnoreDstHour => &mut form.ignore_dst_hour,
        Flag::Deletes | Flag::Permanent | Flag::AskOverwrite => {
            let Some(confirm) = dialog.confirm.as_mut() else {
                return;
            };
            match flag {
                Flag::Deletes => &mut confirm.deletes,
                Flag::Permanent => &mut confirm.permanent,
                _ => &mut confirm.ask_overwrite,
            }
        }
    };
    *target = on;
}
