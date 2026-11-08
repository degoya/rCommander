//! Local port forwards of SSH connections (RC-110-03).
//!
//! Saving and starting a forward is a person's decision (`GuiOnly`): a listening port reaches
//! every process and user of this computer, and through it the target. An agent lists, stops
//! and deletes; the decision about the port and the target is made already.

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{ConnectionId, ForwardDraft};

/// The parameters of `forwards.save`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveForwardRequest {
    /// The SSH connection the forward belongs to.
    pub connection_id: ConnectionId,
    /// The forward to change; a new one when `None`.
    #[serde(default)]
    pub id: Option<i64>,
    pub forward: ForwardDraft,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "forwards.delete",
        class: CommandClass::Write,
        summary: "Delete a saved port forward of a connection, stopping it first when it runs",
    },
    CommandSpec {
        id: "forwards.list",
        class: CommandClass::Read,
        summary: "A connection's local port forwards with whether each runs, or every running one",
    },
    CommandSpec {
        id: "forwards.save",
        class: CommandClass::GuiOnly,
        summary: "Add or change a local port forward of an SSH connection: its port here and its target",
    },
    CommandSpec {
        id: "forwards.start",
        class: CommandClass::GuiOnly,
        summary: "Start a local port forward: listen on its port and carry each connection over the SSH session",
    },
    CommandSpec {
        id: "forwards.stop",
        class: CommandClass::Write,
        summary: "Stop a running port forward: its listener and every connection through it close",
    },
    CommandSpec {
        id: "forwards.stop_all",
        class: CommandClass::Write,
        summary: "Stop every running port forward, or those of one connection",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "forwards.delete" => DeleteForward { id: i64 },
            "forwards.list" => ListForwards {
                /// The connection whose forwards to list; every running one when `None`.
                #[serde(default)]
                connection_id: Option<ConnectionId>,
            },
            "forwards.save" => SaveForward(SaveForwardRequest),
            "forwards.start" => StartForward { id: i64 },
            "forwards.stop" => StopForward { id: i64 },
            "forwards.stop_all" => StopAllForwards {
                /// Only this connection's; every running forward when `None`.
                #[serde(default)]
                connection_id: Option<ConnectionId>,
            },
        } }
    };
}
pub(super) use commands;
