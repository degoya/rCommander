//! The `queue` tool group: the operations queue as the queue window steers it (RC-010-14) - its
//! order with lanes and places, waiting jobs moved or removed, everything paused or resumed, the
//! finished jobs cleared. Single jobs keep their `ops_pause`, `ops_resume` and `ops_cancel`.
//! Nothing here deletes a file: removing a waiting job only keeps it from starting. The jobs run
//! at once (a copy, move or delete confirmed with OK) are listed too, marked `at_once`; they are
//! paused, resumed and cancelled, never moved, put into a list or saved (RC-010-25).
//!
//! The queue's named lists (RC-010-15): made, renamed, deleted, started, stopped, a job moved
//! between them, a list saved as a transfer list and loaded again. Loading is a write - the jobs
//! wait in a stopped list until queue_start - and destructive when a job of the list overwrites
//! (`Engine::effective_class`); every place a job of it writes to passes the write guard first
//! (`guard_write.rs`, RC-080-42). Deleting a saved list is destructive.
//!
//! Several jobs at once, as the queue window's selection steers them (RC-010-15): moved keeping
//! their order, put into a list or a new one, taken off, a list or every list cleared. Taking a
//! started job off cancels it as ops_cancel does, so these are writes like ops_cancel.

use rc_engine::{Command, Reorder};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
};

/// Where a waiting job goes among the waiting jobs of its lane.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StepArg {
    /// One place nearer the start.
    Up,
    /// One place further back.
    Down,
    /// First: it starts as soon as its lane is free.
    Front,
    /// Last: every other waiting job of its lane starts first.
    Back,
}

impl From<StepArg> for Reorder {
    fn from(arg: StepArg) -> Self {
        match arg {
            StepArg::Up => Self::Up,
            StepArg::Down => Self::Down,
            StepArg::Front => Self::Front,
            StepArg::Back => Self::Back,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveParams {
    /// The waiting job's id, as queue_list and ops_list answer it.
    pub job: u64,
    /// up, down, front or back.
    pub step: StepArg,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveManyParams {
    /// The jobs' ids; the waiting ones move, keeping their order among themselves.
    pub jobs: Vec<u64>,
    /// up, down, front or back.
    pub step: StepArg,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct JobsParams {
    /// The jobs' ids, as queue_list answers them.
    pub jobs: Vec<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct AssignManyParams {
    /// The jobs' ids.
    pub jobs: Vec<u64>,
    /// The list they go into.
    pub list: u64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ClearParams {
    /// The list to clear, as queue_lists answers it; 0 is the default list.
    #[serde(default)]
    pub list: u64,
    /// Clear every list instead of one.
    #[serde(default)]
    pub all_lists: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct RemoveParams {
    /// The waiting job's id.
    pub job: u64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct NameParams {
    /// The list's name: not blank, at most 100 characters.
    pub name: String,
    /// Jobs to put into the new list at once, as queue_assign_many puts them.
    #[serde(default)]
    pub jobs: Vec<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// The list's id, as queue_lists answers it; 0 is the default list.
    pub list: u64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct RenameListParams {
    pub list: u64,
    /// The new name.
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct AssignParams {
    /// The job's id.
    pub job: u64,
    /// The list it goes into.
    pub list: u64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SaveParams {
    pub list: u64,
    /// The saved list's name; the list's own name when omitted. A saved list of that name is
    /// replaced.
    #[serde(default)]
    pub name: Option<String>,
    /// Load it again, stopped, when rCommander starts.
    #[serde(default)]
    pub restore_on_start: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SavedParams {
    /// The saved list's id, as queue_lists answers it under saved.
    pub saved: i64,
}

#[tool_router(router = queue_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "The operations queue in the order the queue window shows it: the jobs run at once first (a copy, move or delete confirmed with OK, which waits for no queue; at_once true), then the running jobs of the lists, then the waiting ones lane by lane in the order they start, then the finished ones. Each entry has the job (as ops_list gives it), its lane (the backend it writes to; one job runs per lane at a time), its place among the waiting jobs of the lane (0 starts next; null when it runs, has finished or waits for no lane), its list and at_once. The window hides the jobs run at once while the setting queue.show_at_once is off; this list always has them. The jobs of one copy, move or delete of several sources (one job per source, setting queue.one_job_per_item) stand side by side and share job.batch."
    )]
    pub(crate) async fn queue_list(&self) -> ToolResult {
        self.dispatch_mcp(Command::QueueList {}).await
    }

    #[tool(
        description = "Move a waiting job up, down, to the front or to the back among the waiting jobs of its lane: the order decides which job starts when the lane is free. Answers with the queue as queue_list does. Errors: job_not_found, job_finished, job_at_once (it runs at once and waits for no place), job_not_waiting (the job runs or waits for no lane)."
    )]
    pub(crate) async fn queue_move(
        &self,
        Parameters(params): Parameters<MoveParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueMove {
            job: params.job,
            step: params.step.into(),
        })
        .await
    }

    #[tool(
        description = "Take a waiting job out of the queue before it starts: it ends as cancelled and leaves the list; no file is touched. A running job, or one run at once, is stopped with ops_cancel instead. Errors: job_not_found, job_finished, job_at_once, job_not_waiting."
    )]
    pub(crate) async fn queue_remove(
        &self,
        Parameters(params): Parameters<RemoveParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueRemove { job: params.job })
            .await
    }

    #[tool(
        description = "Move several jobs at once up, down, to the front or to the back of their lanes, keeping their order among themselves; the ones that run, run at once or have finished stay where they are. Answers with the queue as queue_list does. Errors: job_not_found (then nothing moves)."
    )]
    pub(crate) async fn queue_move_many(
        &self,
        Parameters(params): Parameters<MoveManyParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueMoveMany {
            jobs: params.jobs,
            step: params.step.into(),
        })
        .await
    }

    #[tool(
        description = "Take several jobs off the queue: a waiting one ends as cancelled without having started, a started one (running, paused, asking) is cancelled as ops_cancel cancels it - the file being written is removed, files already complete stay -, a finished one is forgotten. None is listed afterwards; jobs of one operation not named keep running. Answers with the queue. Errors: job_not_found (then nothing is taken off)."
    )]
    pub(crate) async fn queue_discard(
        &self,
        Parameters(params): Parameters<JobsParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueDiscard { jobs: params.jobs })
            .await
    }

    #[tool(
        description = "Clear a list of the queue (list, 0 the default list) or every list (all_lists): each of its jobs is taken off as queue_discard takes it, finished or not. Jobs run at once are in no list and stay. Answers with the queue. Errors: queue_list_not_found."
    )]
    pub(crate) async fn queue_clear(
        &self,
        Parameters(params): Parameters<ClearParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueClear {
            list: params.list,
            all_lists: params.all_lists,
        })
        .await
    }

    #[tool(
        description = "Pause every unfinished job, the ones run at once among them: running ones between two buffers, waiting ones before they start. Answers with the queue."
    )]
    pub(crate) async fn queue_pause_all(&self) -> ToolResult {
        self.dispatch_mcp(Command::QueuePauseAll {}).await
    }

    #[tool(
        description = "Resume every paused job, the ones run at once among them; free lanes start their next job. Answers with the queue."
    )]
    pub(crate) async fn queue_resume_all(&self) -> ToolResult {
        self.dispatch_mcp(Command::QueueResumeAll {}).await
    }

    #[tool(
        description = "Take the finished jobs (completed, cancelled, failed) off the queue's list. Answers with the queue."
    )]
    pub(crate) async fn queue_clear_finished(&self) -> ToolResult {
        self.dispatch_mcp(Command::QueueClearFinished {}).await
    }

    #[tool(
        description = "The queue's lists - each with id, name (empty for the default list 0), started and its unfinished job count - and the saved transfer lists (id, name, restore_on_start, saved_at, entries). Every job of queue_list names its list. Lists share the lanes: two lists writing to one disk or server take turns, lists on different targets run side by side."
    )]
    pub(crate) async fn queue_lists(&self) -> ToolResult {
        self.dispatch_mcp(Command::QueueLists {}).await
    }

    #[tool(
        description = "Make a new list of the queue, empty or with the jobs named (jobs); it starts stopped, so it can be filled first (queue_assign), then queue_start runs it. Answers with the lists. Errors: queue_name_invalid, job_not_found, job_at_once (a job run at once goes into no list; then no list is made)."
    )]
    pub(crate) async fn queue_create(
        &self,
        Parameters(params): Parameters<NameParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueCreate {
            name: params.name,
            jobs: params.jobs,
        })
        .await
    }

    #[tool(
        description = "Name a list of the queue anew. Errors: queue_list_not_found, queue_list_fixed (the default list), queue_name_invalid."
    )]
    pub(crate) async fn queue_rename(
        &self,
        Parameters(params): Parameters<RenameListParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueRename {
            list: params.list,
            name: params.name,
        })
        .await
    }

    #[tool(
        description = "Remove a list of the queue; its unfinished jobs go to the default list, nothing is cancelled and no file is touched. Errors: queue_list_not_found, queue_list_fixed."
    )]
    pub(crate) async fn queue_delete(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueDelete { list: params.list })
            .await
    }

    #[tool(
        description = "Start a list: its jobs run as their lanes free up, its paused ones resume. Errors: queue_list_not_found."
    )]
    pub(crate) async fn queue_start(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueStart { list: params.list })
            .await
    }

    #[tool(
        description = "Stop a list: its jobs that have not started stay queued and let other lists pass, its running ones pause. Errors: queue_list_not_found."
    )]
    pub(crate) async fn queue_pause(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueuePause { list: params.list })
            .await
    }

    #[tool(
        description = "Put a job into another list: held while that list is stopped if it has not started; a running job runs on. Answers with the queue as queue_list does. Errors: job_not_found, job_finished, job_at_once (a job run at once goes into no list), queue_list_not_found."
    )]
    pub(crate) async fn queue_assign(
        &self,
        Parameters(params): Parameters<AssignParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueAssign {
            job: params.job,
            list: params.list,
        })
        .await
    }

    #[tool(
        description = "Put several jobs into another list at once: one that has not started is held while that list is stopped, a running one runs on, a finished one is shown there. Answers with the queue as queue_list does. Errors: job_not_found, job_at_once, queue_list_not_found (then nothing moves)."
    )]
    pub(crate) async fn queue_assign_many(
        &self,
        Parameters(params): Parameters<AssignManyParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueAssignMany {
            jobs: params.jobs,
            list: params.list,
        })
        .await
    }

    #[tool(
        description = "Save a list's unfinished copies and moves as a transfer list to load later: sources, target, kind and options, connections by id only, never a secret. Other jobs, and jobs on a quick connection or in an archive, are left out and named (left_out); the jobs run at once are in no list and never saved. Errors: queue_list_not_found, queue_name_invalid, queue_list_empty, database_unavailable."
    )]
    pub(crate) async fn queue_save(
        &self,
        Parameters(params): Parameters<SaveParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueSave {
            list: params.list,
            name: params.name,
            restore_on_start: params.restore_on_start,
        })
        .await
    }

    #[tool(
        description = "Load a saved transfer list into a new, stopped list named like it: its jobs wait until queue_start. A list with a job that overwrites needs a confirmation code. Answers with the list, its jobs and the entries not loaded (position, code: connection_not_found when the connection was deleted, invalid_entry, sensitive_path when a source is or holds a key or credential file, or a database error). A job that writes into rCommander's data folder, an SSH folder, an autostart folder or a shell's start-up file refuses the whole load with protected_target. Errors: saved_queue_not_found, database_unavailable, protected_target."
    )]
    pub(crate) async fn queue_load(
        &self,
        Parameters(params): Parameters<SavedParams>,
    ) -> ToolResult {
        let places = match self.context.engine.saved_queue_places(params.saved).await {
            Ok(places) => places,
            Err(error) => return respond::<()>(Err(error)),
        };
        let guard = self.write_guard();
        for (source, place) in &places {
            if let Some(refusal) = guard.refuse_copy(source, place).await {
                return Ok(refusal);
            }
        }
        self.dispatch_mcp(Command::QueueLoad {
            saved: params.saved,
        })
        .await
    }

    #[tool(
        description = "Delete a saved transfer list for good; the queue's lists and every file stay. Answers with the lists. Errors: saved_queue_not_found."
    )]
    pub(crate) async fn queue_delete_saved(
        &self,
        Parameters(params): Parameters<SavedParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::QueueDeleteSaved {
            saved: params.saved,
        })
        .await
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
