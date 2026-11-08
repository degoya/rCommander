//! The samples of the port forwards' commands (RC-110-03) and the remote edits' (RC-110-01);
//! `registry_samples_more.rs` is full.

use crate::{Command, ConnectionId};

/// One sample of each port-forward command.
pub(super) fn forwards() -> Vec<Command> {
    vec![
        Command::DeleteForward { id: 1 },
        Command::ListForwards {
            connection_id: Some(ConnectionId(1)),
        },
        Command::SaveForward(crate::SaveForwardRequest {
            connection_id: ConnectionId(1),
            id: None,
            forward: crate::ForwardDraft {
                bind_address: "127.0.0.1".into(),
                bind_port: 15432,
                target_host: "db".into(),
                target_port: 5432,
                auto_start: true,
                name: None,
            },
        }),
        Command::StartForward { id: 1 },
        Command::StopForward { id: 1 },
        Command::StopAllForwards {
            connection_id: None,
        },
    ]
}

/// One sample of each command of the server files edited through copies (RC-110-01).
pub(super) fn remote_edits() -> Vec<Command> {
    let id = crate::RemoteEditId(1);
    vec![
        Command::ListRemoteEdits {},
        Command::UploadRemoteEdit {
            id,
            choice: crate::UploadChoice::AsCopy,
            always: false,
        },
        Command::SkipRemoteEdit { id },
        Command::CloseRemoteEdit { id },
        Command::RemoteEditWindow {},
    ]
}
