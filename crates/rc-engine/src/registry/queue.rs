//! The queue window: the jobs of the operations queue in their order, the waiting ones reordered
//! or removed, all paused or resumed at once (RC-010-14). Single jobs are paused, resumed and
//! cancelled with `ops.pause`, `ops.resume` and `ops.cancel`. The queue's named lists are made,
//! started, stopped, saved and loaded here too (RC-010-15), and the jobs selected in the window
//! are moved, put into a list or taken off together, a list or every list cleared.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "queue.assign",
        class: CommandClass::Write,
        summary: "Put a job into another list of the queue; a stopped list holds it until it starts",
    },
    CommandSpec {
        id: "queue.assign_many",
        class: CommandClass::Write,
        summary: "Put several jobs into another list of the queue at once",
    },
    CommandSpec {
        id: "queue.clear",
        class: CommandClass::Write,
        summary: "Take every job of a list, or of every list, off the queue: waiting ones removed, started ones cancelled, finished ones forgotten",
    },
    CommandSpec {
        id: "queue.clear_finished",
        class: CommandClass::Write,
        summary: "Take the finished jobs off the queue's list",
    },
    CommandSpec {
        id: "queue.create",
        class: CommandClass::Write,
        summary: "Make a new list of the queue under a name, empty or with the jobs named; it starts stopped",
    },
    CommandSpec {
        id: "queue.delete",
        class: CommandClass::Write,
        summary: "Remove a list of the queue; its jobs go to the default list, nothing is cancelled",
    },
    CommandSpec {
        id: "queue.delete_saved",
        class: CommandClass::Destructive,
        summary: "Delete a saved transfer list for good",
    },
    CommandSpec {
        id: "queue.discard",
        class: CommandClass::Write,
        summary: "Take several jobs off the queue: waiting ones removed, started ones cancelled (the file being written is removed), finished ones forgotten",
    },
    CommandSpec {
        id: "queue.list",
        class: CommandClass::Read,
        summary: "The operations queue in its order: each job with its lane and its place among the waiting ones",
    },
    CommandSpec {
        id: "queue.lists",
        class: CommandClass::Read,
        summary: "The lists of the queue with their state and job count, and the saved transfer lists",
    },
    CommandSpec {
        id: "queue.load",
        class: CommandClass::Write,
        summary: "Load a saved transfer list into a new, stopped list of the queue; entries that cannot be loaded are named",
    },
    CommandSpec {
        id: "queue.move",
        class: CommandClass::Write,
        summary: "Move a waiting job up, down, to the front or to the back of its lane: the order decides which starts next",
    },
    CommandSpec {
        id: "queue.move_many",
        class: CommandClass::Write,
        summary: "Move several waiting jobs up, down, to the front or to the back of their lanes, keeping their order",
    },
    CommandSpec {
        id: "queue.pause",
        class: CommandClass::Write,
        summary: "Stop a list of the queue: its waiting jobs are held, its running ones pause",
    },
    CommandSpec {
        id: "queue.pause_all",
        class: CommandClass::Write,
        summary: "Pause every job of the queue, running or waiting",
    },
    CommandSpec {
        id: "queue.remove",
        class: CommandClass::Write,
        summary: "Take a waiting job out of the queue before it starts",
    },
    CommandSpec {
        id: "queue.rename",
        class: CommandClass::Write,
        summary: "Name a list of the queue anew",
    },
    CommandSpec {
        id: "queue.resume_all",
        class: CommandClass::Write,
        summary: "Resume every paused job of the queue",
    },
    CommandSpec {
        id: "queue.save",
        class: CommandClass::Write,
        summary: "Save a list's copies and moves as a transfer list to load later (paths and connection ids, no secrets)",
    },
    CommandSpec {
        id: "queue.start",
        class: CommandClass::Write,
        summary: "Start a list of the queue: its jobs run as their lanes free up",
    },
    CommandSpec {
        id: "queue.window",
        class: CommandClass::Read,
        summary: "Open the queue window: every job with its state, progress and lane",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// A job into another list; one that runs runs on.
            "queue.assign" => QueueAssign { job: JobId, list: QueueListId },
            /// The jobs into another list at once, a finished one too (RC-010-15).
            "queue.assign_many" => QueueAssignMany { jobs: Vec<JobId>, list: QueueListId },
            /// The jobs of `list` (the default list unless named), or of every list with
            /// `all_lists`, taken off as `queue.discard` takes them; the jobs run at once stay.
            "queue.clear" => QueueClear {
                #[serde(default)]
                list: QueueListId,
                #[serde(default)]
                all_lists: bool,
            },
            "queue.clear_finished" => QueueClearFinished {},
            /// With `jobs`, they go into the new list as `queue.assign_many` puts them.
            "queue.create" => QueueCreate {
                name: String,
                #[serde(default)]
                jobs: Vec<JobId>,
            },
            /// The list's unfinished jobs go to the default list, which cannot be deleted.
            "queue.delete" => QueueDelete { list: QueueListId },
            "queue.delete_saved" => QueueDeleteSaved { saved: i64 },
            /// Waiting jobs end as cancelled without having started, started ones are cancelled
            /// as `ops.cancel` cancels them, finished ones are forgotten; none is listed after.
            "queue.discard" => QueueDiscard { jobs: Vec<JobId> },
            "queue.list" => QueueList {},
            "queue.lists" => QueueLists {},
            /// Into a new list named like the saved one, stopped until `queue.start`.
            "queue.load" => QueueLoad { saved: i64 },
            /// A waiting job one place up or down among the waiting jobs of its lane, first or last.
            "queue.move" => QueueMove { job: JobId, step: Reorder },
            /// The waiting ones among the jobs, keeping their order; the others stay.
            "queue.move_many" => QueueMoveMany { jobs: Vec<JobId>, step: Reorder },
            "queue.pause" => QueuePause { list: QueueListId },
            "queue.pause_all" => QueuePauseAll {},
            /// A waiting job ends as cancelled without having started; a running one is cancelled
            /// with `ops.cancel`.
            "queue.remove" => QueueRemove { job: JobId },
            "queue.rename" => QueueRename { list: QueueListId, name: String },
            "queue.resume_all" => QueueResumeAll {},
            /// Under `name`, else the list's own name; a saved list of that name is replaced.
            "queue.save" => QueueSave {
                list: QueueListId,
                #[serde(default)]
                name: Option<String>,
                /// Loaded again, stopped, when the app starts.
                #[serde(default)]
                restore_on_start: bool,
            },
            "queue.start" => QueueStart { list: QueueListId },
            /// Ctrl+Shift+Q and a click on the background bar: the window opens the queue window.
            "queue.window" => QueueWindow {},
        } }
    };
}
pub(super) use commands;
