//! Login scripts and the script each connection runs (RC-050-08).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "login_scripts.assign",
        class: CommandClass::Exec,
        summary: "Set the login script a connection types after it logged in, or none",
    },
    CommandSpec {
        id: "login_scripts.clear_secret",
        class: CommandClass::GuiOnly,
        summary: "Remove a secret stored with a login script",
    },
    CommandSpec {
        id: "login_scripts.create",
        class: CommandClass::Write,
        summary: "Add a login script: steps that wait for prompts and send text or stored secrets",
    },
    CommandSpec {
        id: "login_scripts.delete",
        class: CommandClass::Destructive,
        summary: "Delete a login script with its secrets; its connections run none",
    },
    CommandSpec {
        id: "login_scripts.get",
        class: CommandClass::Read,
        summary: "One login script: steps, timeouts, its secrets by label, its connections",
    },
    CommandSpec {
        id: "login_scripts.list",
        class: CommandClass::Read,
        summary: "Every login script, by name, without secret values",
    },
    CommandSpec {
        id: "login_scripts.set_secret",
        class: CommandClass::GuiOnly,
        summary: "Store a secret encrypted with a login script, under a label",
    },
    CommandSpec {
        id: "login_scripts.update",
        class: CommandClass::Write,
        summary: "Change a login script's name, steps or timeouts (no secrets)",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "login_scripts.list" => ListLoginScripts,
            "login_scripts.get" => GetLoginScript { id: LoginScriptId },
            "login_scripts.create" => CreateLoginScript { script: LoginScriptDraft },
            "login_scripts.update" => UpdateLoginScript {
                id: LoginScriptId,
                patch: LoginScriptPatch,
            },
            "login_scripts.delete" => DeleteLoginScript { id: LoginScriptId },
            /// The script the connection runs after it logged in; none when `script_id` is `None`.
            "login_scripts.assign" => AssignLoginScript {
                connection_id: ConnectionId,
                #[serde(default)]
                script_id: Option<LoginScriptId>,
            },
            "login_scripts.set_secret" => SetLoginScriptSecret {
                script_id: LoginScriptId,
                label: String,
                value: SecretParam,
            },
            "login_scripts.clear_secret" => ClearLoginScriptSecret {
                script_id: LoginScriptId,
                secret_id: i64,
            },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "login_scripts_tests.rs"]
mod tests;
