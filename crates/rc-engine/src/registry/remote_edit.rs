//! The server files edited through watched copies (RC-110-01): F4, Shift+F4, Enter and F3 with
//! an external viewer open them through `file.open_external` and `file.open`. Their list
//! reaches MCP; uploading, skipping and closing act on a person's editor session and are the
//! window's alone, as is the window itself.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "remote_edit.close",
        class: CommandClass::GuiOnly,
        summary: "Stop editing a server file: its local copy goes (Done, Discard)",
    },
    CommandSpec {
        id: "remote_edit.list",
        class: CommandClass::Read,
        summary: "The server files being edited through local copies: name, server, path, state and the question waiting, never their content",
    },
    CommandSpec {
        id: "remote_edit.skip",
        class: CommandClass::GuiOnly,
        summary: "Leave a server file's change in its copy for now (Not now, Cancel, Later)",
    },
    CommandSpec {
        id: "remote_edit.upload",
        class: CommandClass::GuiOnly,
        summary: "Upload a server file's copy: checked first, over the server's file, as a copy beside it or in place",
    },
    CommandSpec {
        id: "remote_edit.window",
        class: CommandClass::GuiOnly,
        summary: "Open the list of server files being edited",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "remote_edit.list" => ListRemoteEdits {},
            /// `choice`: how - checked first (the default), over the server's file, beside it,
            /// in place. `always`: the question's box "Upload changes without asking" - from now
            /// on `remote_edit.upload` is `auto`.
            "remote_edit.upload" => UploadRemoteEdit {
                id: RemoteEditId,
                #[serde(default)]
                choice: UploadChoice,
                #[serde(default)]
                always: bool,
            },
            "remote_edit.skip" => SkipRemoteEdit { id: RemoteEditId },
            "remote_edit.close" => CloseRemoteEdit { id: RemoteEditId },
            "remote_edit.window" => RemoteEditWindow {},
        } }
    };
}
pub(super) use commands;
