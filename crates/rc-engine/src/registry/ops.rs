//! The operations queue: copy, move, delete, mkdir as jobs (RC-010-06).

use rc_ops::{DeleteMode, ErrorPolicy, JobOptions, QueueListId};
use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::PanelId;

/// The parameters of `ops.copy` and `ops.move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRequest {
    /// The panel the sources are in; the target is the other one's.
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// Absolute, relative to the panel's directory, or `sftp://<connection>/<path>`. Empty: the
    /// panel's selection, or the entry under its cursor.
    #[serde(default)]
    pub sources: Vec<String>,
    /// The directory to copy into: absolute, relative to the other panel's directory, or
    /// `sftp://<connection>/<path>`; `None` is the other panel's directory.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub options: JobOptions,
}

/// The parameters of `ops.delete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// As for `ops.copy`.
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub mode: DeleteMode,
    #[serde(default)]
    pub on_error: ErrorPolicy,
    /// The queue list the delete goes into (F2 in the delete dialog, RC-010-15); `None` runs it
    /// at once, outside the queue's lists, as Total Commander's OK does.
    #[serde(default)]
    pub queue: Option<QueueListId>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "ops.answer",
        class: CommandClass::Destructive,
        summary: "Answer the question a job waits on: a conflict or a failing file",
    },
    CommandSpec {
        id: "ops.cancel",
        class: CommandClass::Write,
        summary: "Cancel a job; the file being written is removed",
    },
    CommandSpec {
        id: "ops.copy",
        class: CommandClass::Write,
        summary: "Copy files and directories into a directory, as a queued job",
    },
    CommandSpec {
        id: "ops.delete",
        class: CommandClass::Destructive,
        summary: "Delete files and directories (recycle bin or permanently) as a queued job",
    },
    CommandSpec {
        id: "ops.list",
        class: CommandClass::Read,
        summary: "The jobs of the operations queue with their state and progress",
    },
    CommandSpec {
        id: "ops.mkdir",
        class: CommandClass::Write,
        summary: "Create a directory and its missing parents, as a queued job",
    },
    CommandSpec {
        id: "ops.move",
        class: CommandClass::Write,
        summary: "Move files and directories into a directory, as a queued job",
    },
    CommandSpec {
        id: "ops.pause",
        class: CommandClass::Write,
        summary: "Pause a job between two buffers",
    },
    CommandSpec {
        id: "ops.resume",
        class: CommandClass::Write,
        summary: "Resume a paused job",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "ops.answer" => OpsAnswer { job: JobId, answer: Answer },
            "ops.cancel" => OpsCancel { job: JobId },
            "ops.copy" => OpsCopy(TransferRequest),
            "ops.delete" => OpsDelete(DeleteRequest),
            "ops.list" => OpsList {
                /// Only this job; every job when `None`.
                #[serde(default)]
                job: Option<JobId>,
            },
            "ops.mkdir" => OpsMkdir {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, relative to the panel's directory, or a server's location as
                /// `ops.copy` takes it (`sftp://<connection>/<path>`, `ftp://...`).
                path: String,
            },
            "ops.move" => OpsMove(TransferRequest),
            "ops.pause" => OpsPause { job: JobId },
            "ops.resume" => OpsResume { job: JobId },
        } }
    };
}
pub(super) use commands;
