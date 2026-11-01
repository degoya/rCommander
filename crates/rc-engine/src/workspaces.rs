//! Workspaces (RC-100-68): what both panels show, saved under a name and called up at once - each
//! panel's tabs with their folders, sort and view, the file list or a terminal (local, WSL, or SSH
//! on a connection), the active panel and the divider, and a colour theme for each mode.
//!
//! The spec is versioned JSON ([`WorkspaceSpec`], `spec.rs`), checked at every boundary: 32 tabs
//! a panel, 64 KiB, connections and themes that exist when it is saved (`store.rs`, rows of
//! `rc_db`'s `workspaces` table). `workspaces.capture` takes what the panels show now
//! (`capture.rs`); `workspaces.apply` makes the panels show a workspace (`apply.rs`): the tabs
//! after the setting [`APPLY_MODE_KEY`] (locked tabs stay by default), a running terminal of the
//! same kind reused or a new one started, a connection deleted since or a server out of reach
//! left out with a note - never blocking the rest. The workspace applied last is this engine's
//! active one: its theme ([`theme_of`]) takes the place of the window's while it is.
//! [`ON_START_KEY`] names the workspace the window applies at the start ([`Engine::start_workspace`]).

mod apply;
mod capture;
mod error;
mod spec;
mod store;
mod view;

use std::sync::{Mutex, PoisonError};

pub use error::WorkspaceError;
pub use spec::{
    ApplyMode, MAX_NAME_CHARS, MAX_SPEC_BYTES, MAX_TABS, MAX_TEXT, PanelSpec, SPEC_VERSION,
    SpecContext, TabSpec, TermSpec, Workspace, WorkspaceId, WorkspaceSpec, WorkspacesOutcome,
    clean_name,
};

use crate::{Engine, settings::themes::ThemeMode};

/// The setting that says what applying a workspace does to the tabs a panel has: `replace_keep_locked`
/// (the default), `replace_all` or `add` ([`ApplyMode`]).
pub const APPLY_MODE_KEY: &str = "workspaces.apply_mode";

/// The setting that names the workspace the window applies at the start, by its id; empty for the
/// tabs of the last run.
pub const ON_START_KEY: &str = "workspaces.on_start";

/// The engine's own state of the workspaces: the one applied last, and the turn of an apply.
#[derive(Default)]
pub(crate) struct WorkspaceDesk {
    active: Mutex<Option<WorkspaceId>>,
    /// Held for a whole `workspaces.apply`: two at once would mix their tabs.
    applying: tokio::sync::Mutex<()>,
}

impl WorkspaceDesk {
    fn active(&self) -> Option<WorkspaceId> {
        *self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_active(&self, id: Option<WorkspaceId>) {
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = id;
    }
}

impl Engine {
    /// The workspace applied last in this window; `None` before the first and after it was
    /// deleted.
    #[must_use]
    pub fn active_workspace(&self) -> Option<WorkspaceId> {
        self.inner.workspaces.active()
    }
}

/// The theme the active workspace of `workspaces` gives the mode `mode`: its id, or `None` when
/// no workspace is active or the active one leaves the mode to the window. A theme deleted since
/// is the window's to drop, as it drops a connection's (RC-100-51).
#[must_use]
pub fn theme_of(workspaces: &[Workspace], mode: ThemeMode) -> Option<&str> {
    let active = workspaces.iter().find(|workspace| workspace.active)?;
    match mode {
        ThemeMode::Light => active.theme_light.as_deref(),
        ThemeMode::Dark => active.theme_dark.as_deref(),
    }
}

/// One command of each workspace variant, for the registry's tests.
#[cfg(test)]
pub(crate) fn one_of_each() -> Vec<crate::Command> {
    use crate::{Command, CreateWorkspaceRequest, UpdateWorkspaceRequest};
    vec![
        Command::ListWorkspaces {},
        Command::GetWorkspace { id: 1 },
        Command::CreateWorkspace(CreateWorkspaceRequest {
            name: "Servers".into(),
            spec: spec::sample(),
            theme_light: None,
            theme_dark: Some("nord-dark".into()),
        }),
        Command::CaptureWorkspace {
            id: None,
            name: Some("Now".into()),
        },
        Command::UpdateWorkspace(UpdateWorkspaceRequest {
            id: 1,
            name: Some("Web".into()),
            spec: Some(spec::sample()),
            theme_light: Some(None),
            theme_dark: Some(Some("nord-dark".into())),
        }),
        Command::RenameWorkspace {
            id: 1,
            name: "Web".into(),
        },
        Command::MoveWorkspace { id: 1, position: 0 },
        Command::DeleteWorkspace { id: 1 },
        Command::ApplyWorkspace { id: 1 },
        Command::WorkspaceManager {},
    ]
}

#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;
