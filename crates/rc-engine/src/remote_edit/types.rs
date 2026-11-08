//! What a remote edit is to the window and to MCP: its id, its state, the question it waits on,
//! the answer to one, and the stat it is held against.

use std::{fmt, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::VfsPath;

/// One server file being edited through a local copy; unique for the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteEditId(pub u64);

impl fmt::Display for RemoteEditId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where an edit stands, as the list of server files being edited shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RemoteEditState {
    /// The copy is as it was fetched or last uploaded.
    Unchanged,
    /// The copy was saved and the change is not on the server yet.
    Changed,
    /// The change goes up now.
    Uploading,
    /// The change is on the server, uploaded at `at`.
    Uploaded { at: SystemTime },
    /// The server's file changed, or went, since the copy was fetched.
    Conflict,
    /// The last upload failed; `code` is the error's code.
    Failed { code: String },
}

impl RemoteEditState {
    /// Whether the copy holds a change the server does not have.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        matches!(
            self,
            Self::Changed | Self::Uploading | Self::Conflict | Self::Failed { .. }
        )
    }
}

/// The question an edit waits on; the window asks it, the answer is `remote_edit.upload` (with
/// the choice the button names) or `remote_edit.skip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteEditQuestion {
    /// The copy was saved (`remote_edit.upload` = `ask`): Upload or Not now.
    Upload,
    /// The server's file changed since it was fetched: Overwrite, Upload as copy or Not now.
    ServerChanged,
    /// The server's file is gone: Upload anyway or Not now.
    ServerGone,
    /// The server's file has several hard links or another owner: Overwrite in place or Cancel.
    InPlace,
    /// A copy kept at the last quit with a change in it: Upload, Discard or Later.
    Kept,
}

/// How `remote_edit.upload` uploads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadChoice {
    /// Upload as the edit's rules say: the server's file checked first (when
    /// `remote_edit.check_server_changes` is on), a file with links asked about.
    #[default]
    Upload,
    /// Replace the server's file without looking at it again ("Overwrite", "Upload anyway").
    Overwrite,
    /// Upload beside it under a free name, `name (2).ext` ("Upload as copy").
    AsCopy,
    /// Write into the server's file itself, without a part file: links and owner stay; a break
    /// may leave half a file ("Overwrite in place").
    InPlace,
}

/// One edit as the list shows it; never the file's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteEditInfo {
    pub id: RemoteEditId,
    /// The file's name.
    pub name: String,
    /// The connection's name.
    pub server: String,
    /// The server file as it was opened (a link stays the link).
    pub path: VfsPath,
    #[serde(flatten)]
    pub state: RemoteEditState,
    /// The question waiting for the person, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<RemoteEditQuestion>,
}

/// What changed about an edit ([`crate::EngineEvent::RemoteEdit`]); `remote_edit.list` has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteEditChange {
    /// A copy was fetched and handed to its program, or a kept one taken up at the start.
    Opened,
    /// Its state changed.
    State,
    /// A question waits for the person.
    Asked,
    /// It is no longer watched; its copy is gone.
    Closed,
}

/// The size and modification time of a file: what a save or a server change is seen by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// The server's file as it was fetched or last uploaded: what a change on the server is seen
/// by, and the Unix mode an upload gives back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub stamp: Stamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

impl Baseline {
    /// The baseline of a server entry.
    #[must_use]
    pub fn of(entry: &rc_vfs::Entry) -> Self {
        Self {
            stamp: Stamp {
                size: entry.size,
                modified: entry.modified,
            },
            mode: entry.attributes.mode,
        }
    }
}
