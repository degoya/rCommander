//! The [`Command`] enum: one variant per registry row, its parameters the variant's fields - or,
//! for a command with many, a request struct of its group's module (`checksum.rs`, `ops.rs`, ...).
//!
//! The variants live with their group: each module of `registry/` lists its own in a
//! `commands!` macro, every variant after its id. `command_enum!` runs through the groups of
//! [`crate::command_groups!`] and builds the enum from all of them, with the id as the serde
//! name and as [`Command::id`]: the id is written once, and no file holds every command.

use rc_ops::{Answer, ApplyTo, JobId, JobOptions, QueueListId, Reorder};
use rc_vfs::{LinkKind, PropertyChange};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    BackupToRequest, ChecksumDialog, ChecksumFilesRequest, ChecksumJob, ChecksumRequest,
    CombineRequest, ConfigExportRequest, ConfigImportRequest, CursorStep, CursorTarget,
    DeleteRequest, ErrorLogRequest, ImportKeyRequest, MaskRequest, MoveConnectionRequest,
    MoveFolderRequest, QuickConnectRequest, RdmImportRequest, RetentionRequest,
    SaveConnectionRequest, SaveCredentialRequest, SearchRequest, SplitRequest,
    SshConfigImportRequest, SshExecRequest, TcImportRequest, TcPasswordsRequest, TransferRequest,
};
use crate::{
    ConnectionDraft, ConnectionId, ConnectionPatch, CredentialId, FolderId, HostKeyAnswer,
    ListMode, LoginScriptDraft, LoginScriptId, LoginScriptPatch, MultiRenameRequest, NameFormat,
    PanelId, SecretKind, SecretParam, SecretSourceRef, SessionPrefs, SortKey, TermId, TermSize,
    compare::FileCompareRequest,
    sync::{DirCompareRequest, SyncApplyRequest},
    viewer::{ReadHexRequest, ReadTextRequest},
};
// The tab, history and hotlist commands (RC-010-03) keep their parameters in structs.
use super::{CommandSpec, spec};
use crate::tabs::requests::{
    AddHotlistFolderRequest, AddHotlistRequest, HistoryJumpRequest, LockTabRequest,
    MoveHotlistRequest, MoveTabRequest, NextTabRequest, OpenHotlistRequest, OpenTabRequest,
    PanelRequest, SelectTabRequest, SwapRequest, TabRequest, TransferDirRequest,
    UpdateHotlistRequest,
};
use crate::{ExternalProgram, FileDialog};
// The colour themes (RC-100-50) keep their model with the settings.
use crate::settings::themes::{ThemeDraft, ThemeMode};

/// Builds [`Command`] and [`Command::id`]: hands the variants gathered so far to the next group's
/// list, and with the last group writes the enum.
macro_rules! command_enum {
    (start, [$($group:ident,)*]) => {
        command_enum!([$($group),*], {});
    };
    ([], {
        $(
            $(#[$attr:meta])*
            $id:literal => $name:ident $({ $($field:tt)* })? $(( $($tuple:tt)* ))?,
        )*
    }) => {
        /// One user action with its parameters. The serde form is `{"command": "<id>", "params":
        /// {...}}`, what a keymap entry stores.
        ///
        /// `panel: None` means the active panel.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "command", content = "params")]
        pub enum Command {
            $(
                $(#[$attr])*
                #[serde(rename = $id)]
                $name $({ $($field)* })? $(( $($tuple)* ))?,
            )*
        }

        impl Command {
            /// The command's registry id.
            #[must_use]
            pub fn id(&self) -> &'static str {
                match self {
                    $(Self::$name { .. } => $id,)*
                }
            }
        }
    };
    ([$group:ident $(, $rest:ident)*], { $($done:tt)* }) => {
        super::$group::commands! { command_enum, [$($rest),*], { $($done)* } }
    };
}

crate::command_groups!(command_enum, start);

impl Command {
    /// The command's registry row.
    #[must_use]
    pub fn spec(&self) -> &'static CommandSpec {
        spec(self.id()).expect("every command has a registry row (registry_tests)")
    }
}
