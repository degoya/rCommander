//! The workspaces (RC-100-68): what both panels show, saved under a name and called up at once.
//! Reading, saving, ordering and deleting them reach MCP; applying one may start shells, so it
//! has the class of `terminal.open`; the workspaces' window (Ctrl+Shift+W) is the window's.

use serde::{Deserialize, Deserializer, Serialize};

use super::{CommandClass, CommandSpec};
use crate::workspaces::{WorkspaceId, WorkspaceSpec};

/// The parameters of `workspaces.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub spec: WorkspaceSpec,
    /// The theme id of the light mode while it is active; the window's when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_light: Option<String>,
    /// The theme id of the dark mode while it is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_dark: Option<String>,
}

/// The parameters of `workspaces.update`: what is given changes, the rest stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub id: WorkspaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<WorkspaceSpec>,
    /// The light mode's theme when it changes: `null` leaves it to the window; left out, it stays.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub theme_light: Option<Option<String>>,
    /// The dark mode's theme, as `theme_light`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub theme_dark: Option<Option<String>>,
}

/// A field that is there, `null` included: with `#[serde(default)]` a missing one stays `None`
/// and `null` reads as `Some(None)`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "workspaces.apply",
        class: CommandClass::Exec,
        summary: "Make both panels show a workspace: its tabs, the file list or a terminal (a shell may start), the active panel, the divider",
    },
    CommandSpec {
        id: "workspaces.capture",
        class: CommandClass::Write,
        summary: "Save what both panels show now as a new workspace, or over the panels of a stored one",
    },
    CommandSpec {
        id: "workspaces.create",
        class: CommandClass::Write,
        summary: "Save a new workspace: a name, what each panel shows, a theme for each mode",
    },
    CommandSpec {
        id: "workspaces.delete",
        class: CommandClass::Destructive,
        summary: "Delete a workspace",
    },
    CommandSpec {
        id: "workspaces.get",
        class: CommandClass::Read,
        summary: "One workspace: its name, what each panel shows, its themes, whether it is active",
    },
    CommandSpec {
        id: "workspaces.list",
        class: CommandClass::Read,
        summary: "The workspaces in their order, the one applied last marked active",
    },
    CommandSpec {
        id: "workspaces.manager",
        class: CommandClass::GuiOnly,
        summary: "Open the workspaces' window",
    },
    CommandSpec {
        id: "workspaces.move",
        class: CommandClass::Write,
        summary: "Give a workspace another place in the list",
    },
    CommandSpec {
        id: "workspaces.rename",
        class: CommandClass::Write,
        summary: "Give a workspace another name",
    },
    CommandSpec {
        id: "workspaces.update",
        class: CommandClass::Write,
        summary: "Change a workspace's name, panels or themes",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "workspaces.list" => ListWorkspaces {},
            "workspaces.get" => GetWorkspace { id: WorkspaceId },
            "workspaces.create" => CreateWorkspace(CreateWorkspaceRequest),
            /// `id`: the panels over that workspace's spec (and `name` renames it); else a new
            /// workspace named `name`.
            "workspaces.capture" => CaptureWorkspace {
                id: Option<WorkspaceId>,
                name: Option<String>,
            },
            "workspaces.update" => UpdateWorkspace(UpdateWorkspaceRequest),
            "workspaces.rename" => RenameWorkspace { id: WorkspaceId, name: String },
            /// `position` counts from 0 among all workspaces; beyond the end is the last place.
            "workspaces.move" => MoveWorkspace { id: WorkspaceId, position: usize },
            "workspaces.delete" => DeleteWorkspace { id: WorkspaceId },
            /// The tabs after `workspaces.apply_mode`; answers the parts left out.
            "workspaces.apply" => ApplyWorkspace { id: WorkspaceId },
            /// Ctrl+Shift+W: the window opens the workspaces' window.
            "workspaces.manager" => WorkspaceManager {},
        } }
    };
}
pub(super) use commands;
