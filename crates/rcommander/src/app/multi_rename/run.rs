//! A run's way through the dialog: Start and Undo ask the engine, the answers land in the
//! dialog, the queued job goes to the operations' bar, and when it is over the panel selects
//! the renamed entries, the table shows their names and the log gets the pairs carried out.

use std::collections::HashMap;

use rc_engine::{Change, Command, JobChange, JobId, JobState, SUBFOLDER_SEPARATOR};

use super::{Answer, Call, Preview, Queued, Running, Status, failure};
use crate::{
    app::{
        App, Effect,
        file_ops::{FileOpsMsg, FollowUp, Submitted},
    },
    i18n::t,
};

impl App {
    /// Start: the run of the previewed input, only without a conflict.
    pub(super) fn mr_start(&mut self) -> Vec<Effect> {
        let Some(dialog) = self
            .multi_rename
            .as_mut()
            .filter(|dialog| dialog.can_start())
        else {
            return Vec::new();
        };
        let Ok(request) = dialog.request() else {
            return Vec::new();
        };
        dialog.pending_pairs = dialog.list.changes();
        dialog.asked = true;
        dialog.status = Status::Busy;
        let apply = Command::MultiRenameApply(request);
        vec![Effect::MultiRename(Call::Run {
            undo: false,
            command: apply,
        })]
    }

    /// Undo: the latest run reversed, this dialog's or an earlier one.
    pub(super) fn mr_undo(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut().filter(|dialog| !dialog.busy()) else {
            return Vec::new();
        };
        dialog.asked = true;
        dialog.status = Status::Busy;
        vec![Effect::MultiRename(Call::Run {
            undo: true,
            command: Command::MultiRenameUndo,
        })]
    }

    pub(super) fn mr_answer(&mut self, answer: Answer) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        match answer {
            Answer::Stale => {}
            Answer::Preview(generation, plan) => {
                if generation == dialog.generation {
                    dialog.preview = match *plan {
                        Ok(plan) => {
                            let conflicts = plan.conflict_count();
                            let changes = plan
                                .items
                                .iter()
                                .filter(|item| item.change != Change::Unchanged)
                                .count();
                            dialog.list.set_items(plan.items);
                            Preview::Ready { changes, conflicts }
                        }
                        Err(error) => Preview::Failed(error),
                    };
                }
            }
            Answer::Presets(Ok((presets, last))) => {
                dialog.presets = presets;
                dialog.last = last;
            }
            Answer::Presets(Err(code)) => dialog.status = failure(code),
            Answer::Queued {
                result: Err(code), ..
            } => {
                dialog.asked = false;
                dialog.pending_pairs.clear();
                dialog.status = failure(code);
            }
            Answer::Queued {
                undo,
                result: Ok(queued),
            } => return self.mr_queued(undo, *queued),
            Answer::Logged(Ok(count)) => {
                dialog.status = Status::Info(t!("multi_rename.logged", count = count));
            }
            Answer::Logged(Err(error)) => {
                dialog.status = Status::Error(t!("multi_rename.log_failed", error = error));
            }
        }
        Vec::new()
    }

    /// The engine queued the run or the undo: its job goes to the operations' bar, which reads
    /// both panels again when it is over.
    fn mr_queued(&mut self, undo: bool, queued: Queued) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        dialog.asked = false;
        let id = queued.job.id;
        let pairs = std::mem::take(&mut dialog.pending_pairs);
        dialog.running = Some(Running {
            job: id,
            undo,
            run: queued.run,
            dir: queued.dir,
            pairs: if undo { Vec::new() } else { pairs },
            before: dialog.list.names().to_vec(),
        });
        let submitted = Submitted {
            panel: dialog.panel,
            follow: FollowUp::Refresh,
            background: true,
            result: Ok(vec![queued.job]),
            refused: None,
        };
        let mut effects = self.on_file_ops(FileOpsMsg::Submitted(Box::new(submitted)));
        // A job over before this answer came: its end was announced already.
        let state = self.engine.ops().job(id).ok().map(|job| job.state);
        if let Some(state) = state.filter(|state| state.is_finished()) {
            effects.extend(self.mr_finished(id, state));
        }
        effects
    }

    /// A change of a job the queue announced: the end of this dialog's run or undo.
    pub(in crate::app) fn multi_rename_job(
        &mut self,
        job: JobId,
        change: &JobChange,
    ) -> Vec<Effect> {
        match change {
            JobChange::Finished { state } => self.mr_finished(job, *state),
            _ => Vec::new(),
        }
    }

    fn mr_finished(&mut self, job: JobId, state: JobState) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        let Some(running) = dialog.running.take_if(|running| running.job == job) else {
            return Vec::new();
        };
        let completed = state == JobState::Completed;
        let mut effects = Vec::new();
        if running.undo {
            if completed && let Some(names) = dialog.undo_names.take() {
                dialog.list.set_names(names);
            }
            dialog.status = if completed {
                Status::Info(t!("multi_rename.undone").to_owned())
            } else {
                Status::Error(t!("multi_rename.undo_stopped").to_owned())
            };
        } else {
            if dialog.log {
                effects.push(Effect::MultiRename(Call::Log {
                    db: self.db.clone(),
                    run: running.run.clone(),
                    dir: running.dir.clone(),
                    pairs: running.pairs.clone(),
                    path: dialog.log_path.trim().to_owned(),
                }));
            }
            if completed {
                let renamed: HashMap<&str, &str> = running
                    .pairs
                    .iter()
                    .map(|(old, new)| (old.as_str(), new.as_str()))
                    .collect();
                // Entries moved into a subfolder have left the folder, and the table.
                let names: Vec<String> = running
                    .before
                    .iter()
                    .map(|name| renamed.get(name.as_str()).copied().unwrap_or(name.as_str()))
                    .filter(|name| !name.contains(SUBFOLDER_SEPARATOR))
                    .map(str::to_owned)
                    .collect();
                if dialog.selected {
                    let panel = Some(dialog.panel);
                    effects.push(Effect::Dispatch(
                        dialog.panel,
                        vec![
                            Command::DeselectAll { panel },
                            Command::ToggleSelection {
                                panel,
                                names: names.clone(),
                            },
                        ],
                    ));
                }
                dialog.list.set_names(names);
                dialog.undo_names = Some(running.before);
                dialog.status = Status::Info(t!("multi_rename.done", count = running.pairs.len()));
            } else {
                dialog.status = Status::Error(t!("multi_rename.stopped").to_owned());
            }
        }
        if !dialog.open {
            self.multi_rename = None;
            return effects;
        }
        effects.extend(self.mr_refresh(false));
        effects
    }
}
