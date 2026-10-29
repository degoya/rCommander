//! Checksums and checksum files (RC-020-05).

use serde::{Deserialize, Serialize};

use super::{CombineRequest, CommandClass, CommandSpec, SplitRequest};
use crate::{Algorithm, PanelId};

/// The parameters of `checksum.compute`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The files, absolute or relative to the panel's directory.
    pub paths: Vec<String>,
    pub algorithm: Algorithm,
}

/// The parameters of `checksum.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumFilesRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The folder of `names`, absolute or relative; the panel's when `None`.
    #[serde(default)]
    pub dir: Option<String>,
    /// Entries of the folder: files, and folders whose files are all included.
    pub names: Vec<String>,
    pub algorithm: Algorithm,
    /// One checksum file beside each file; otherwise one for all.
    #[serde(default)]
    pub per_file: bool,
    /// The one file's name; `<file>.<ext>` for one selected file, else `checksums.<ext>`.
    #[serde(default)]
    pub file_name: Option<String>,
}

/// One of the window's checksum dialogs: Total Commander's "Create Checksum File(s)", "Verify
/// Checksums", "Split File" and "Combine Files".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumDialog {
    Create,
    Verify,
    Split,
    Combine,
}

/// The parameters of `checksum.queue`: what one of the dialogs runs, as a job of the operations
/// queue. A split's or combine's `target_dir` is resolved against the other panel's folder,
/// which the dialog offers (`file.split` and `file.combine` resolve it against the panel's own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChecksumJob {
    /// What `checksum.create` does.
    Create(ChecksumFilesRequest),
    /// What `checksum.verify` does.
    Verify {
        #[serde(default)]
        panel: Option<PanelId>,
        /// The checksum file, absolute or relative.
        path: String,
    },
    /// What `file.split` does.
    Split(SplitRequest),
    /// What `file.combine` does.
    Combine(CombineRequest),
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "checksum.compute",
        class: CommandClass::Read,
        summary: "The digest (CRC32, MD5, SHA-1, SHA-256, SHA-512, BLAKE3) of one or more files",
    },
    CommandSpec {
        id: "checksum.create",
        class: CommandClass::Write,
        summary: "Write checksum files for files and folders: one per file or one for all",
    },
    CommandSpec {
        id: "checksum.dialog",
        class: CommandClass::Read,
        summary: "Open a checksum dialog in the window: create, verify, split or combine",
    },
    CommandSpec {
        id: "checksum.queue",
        class: CommandClass::Write,
        summary: "Run what a checksum dialog asks (create, verify, split, combine) as a queue job",
    },
    CommandSpec {
        id: "checksum.verify",
        class: CommandClass::Read,
        summary: "Check the files a checksum file lists: ok, mismatch, missing per line",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "checksum.compute" => ComputeChecksums(ChecksumRequest),
            "checksum.create" => CreateChecksumFiles(ChecksumFilesRequest),
            /// The window opens the dialog for the panel's selection.
            "checksum.dialog" => ChecksumDialog {
                #[serde(default)]
                panel: Option<PanelId>,
                dialog: ChecksumDialog,
            },
            /// The window's form of the four: a job with pause, cancel and progress; its
            /// result is the finished job's report. Refused under MCP.
            "checksum.queue" => QueueChecksumJob(ChecksumJob),
            "checksum.verify" => VerifyChecksumFile {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The checksum file, absolute or relative.
                path: String,
            },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "checksum_tests.rs"]
mod tests;
