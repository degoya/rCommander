//! The stored workspaces (RC-100-68): `workspaces.list`, `.get`, `.create`, `.update`, `.rename`,
//! `.move` and `.delete` over `rc_db`'s `workspaces` table.
//!
//! What goes in is checked here as at any boundary: the name ([`clean_name`], unique without
//! regard to case), the spec against the connections that exist now, the themes against the
//! themes this system offers, each of its mode. What comes back is read as untrusted too: a row
//! whose spec no longer reads is left out of the list with a warning. Every change announces
//! [`EngineEvent::WorkspacesChanged`].

use rc_db::{DbError, StoredWorkspace, WorkspacePatch};
use serde_json::Value;

use super::{
    SpecContext, Workspace, WorkspaceError, WorkspaceId, WorkspaceSpec, WorkspacesOutcome,
    clean_name,
};
use crate::{
    ConnectionId, CreateWorkspaceRequest, Engine, EngineError, EngineEvent, Outcome,
    UpdateWorkspaceRequest,
    settings::themes::{ThemeMode, load},
};

/// A `workspaces.*` answer.
pub(super) fn answer(outcome: WorkspacesOutcome) -> Outcome {
    Outcome::Workspaces(Box::new(outcome))
}

/// The row `stored` as a workspace; `active` marks the one applied last.
fn workspace_of(
    stored: StoredWorkspace,
    active: Option<WorkspaceId>,
) -> Result<Workspace, WorkspaceError> {
    let spec = WorkspaceSpec::parse(&stored.spec)?;
    Ok(Workspace {
        id: stored.id,
        active: active == Some(stored.id),
        name: stored.name,
        spec,
        theme_light: stored.theme_light,
        theme_dark: stored.theme_dark,
    })
}

/// A theme id as kept: `None` for none or an empty one.
fn theme_id(id: Option<String>) -> Option<String> {
    id.map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
}

/// A failed write: a name another workspace has, or the database's.
fn write_error(error: DbError, name: &str) -> EngineError {
    if error.is_unique_violation() {
        WorkspaceError::NameTaken {
            name: name.to_owned(),
        }
        .into()
    } else {
        EngineError::Database(error)
    }
}

impl Engine {
    /// Every workspace in the person's order, the active one marked; a row whose spec does not
    /// read is left out with a warning.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails.
    pub async fn workspaces(&self) -> Result<Vec<Workspace>, EngineError> {
        let active = self.active_workspace();
        let stored = self.db()?.workspaces().await?;
        Ok(stored
            .into_iter()
            .filter_map(|row| {
                let id = row.id;
                workspace_of(row, active)
                    .map_err(|error| {
                        tracing::warn!(id, code = error.code(), "a stored workspace is left out");
                    })
                    .ok()
            })
            .collect())
    }

    /// `workspaces.list`.
    pub(crate) async fn list_workspaces(&self) -> Result<Outcome, EngineError> {
        let workspaces = self.workspaces().await?;
        Ok(answer(WorkspacesOutcome::List { workspaces }))
    }

    /// `workspaces.get`.
    pub(crate) async fn get_workspace(&self, id: WorkspaceId) -> Result<Outcome, EngineError> {
        let workspace = self.read_workspace(id).await?;
        Ok(answer(WorkspacesOutcome::Workspace(Box::new(workspace))))
    }

    /// The workspace `id`, read and its spec parsed.
    pub(super) async fn read_workspace(&self, id: WorkspaceId) -> Result<Workspace, EngineError> {
        let stored = self
            .db()?
            .workspace(id)
            .await?
            .ok_or(WorkspaceError::Unknown { id })?;
        Ok(workspace_of(stored, self.active_workspace())?)
    }

    /// `workspaces.create`: a new workspace after the last one.
    pub(crate) async fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> Result<Outcome, EngineError> {
        let name = clean_name(&request.name)?;
        let spec = self.checked_spec(&request.spec).await?;
        let light = theme_id(request.theme_light);
        let dark = theme_id(request.theme_dark);
        self.check_themes(light.as_deref(), dark.as_deref()).await?;
        self.require_free_name(&name, None).await?;
        let stored = self
            .db()?
            .create_workspace(&name, &spec, light.as_deref(), dark.as_deref())
            .await
            .map_err(|error| write_error(error, &name))?;
        self.publish(EngineEvent::WorkspacesChanged);
        Ok(answer(WorkspacesOutcome::Saved { id: stored.id }))
    }

    /// `workspaces.update`: what the request names changes, the rest stays.
    pub(crate) async fn update_workspace(
        &self,
        request: UpdateWorkspaceRequest,
    ) -> Result<Outcome, EngineError> {
        let id = request.id;
        let current = self
            .db()?
            .workspace(id)
            .await?
            .ok_or(WorkspaceError::Unknown { id })?;
        let name = request.name.as_deref().map(clean_name).transpose()?;
        let spec = match &request.spec {
            Some(spec) => Some(self.checked_spec(spec).await?),
            None => None,
        };
        let theme_light = request.theme_light.map(theme_id);
        let theme_dark = request.theme_dark.map(theme_id);
        // The themes the workspace has after the change, each checked.
        let light = theme_light.clone().unwrap_or(current.theme_light);
        let dark = theme_dark.clone().unwrap_or(current.theme_dark);
        self.check_themes(light.as_deref(), dark.as_deref()).await?;
        if let Some(name) = &name {
            self.require_free_name(name, Some(id)).await?;
        }
        let patch = WorkspacePatch {
            name: name.clone(),
            spec,
            theme_light,
            theme_dark,
        };
        let shown = name.unwrap_or(current.name);
        self.db()?
            .update_workspace(id, &patch)
            .await
            .map_err(|error| write_error(error, &shown))?
            .ok_or(WorkspaceError::Unknown { id })?;
        self.publish(EngineEvent::WorkspacesChanged);
        Ok(answer(WorkspacesOutcome::Done {}))
    }

    /// `workspaces.rename`.
    pub(crate) async fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: String,
    ) -> Result<Outcome, EngineError> {
        self.update_workspace(UpdateWorkspaceRequest {
            id,
            name: Some(name),
            ..UpdateWorkspaceRequest::default()
        })
        .await
    }

    /// `workspaces.move`: the workspace takes place `position` (from 0) in the list.
    pub(crate) async fn move_workspace(
        &self,
        id: WorkspaceId,
        position: usize,
    ) -> Result<Outcome, EngineError> {
        if !self.db()?.move_workspace(id, position).await? {
            return Err(WorkspaceError::Unknown { id }.into());
        }
        self.publish(EngineEvent::WorkspacesChanged);
        Ok(answer(WorkspacesOutcome::Done {}))
    }

    /// `workspaces.delete`: the active one deleted leaves no workspace active.
    pub(crate) async fn delete_workspace(&self, id: WorkspaceId) -> Result<Outcome, EngineError> {
        if !self.db()?.delete_workspace(id).await? {
            return Err(WorkspaceError::Unknown { id }.into());
        }
        if self.active_workspace() == Some(id) {
            self.inner.workspaces.set_active(None);
        }
        self.publish(EngineEvent::WorkspacesChanged);
        Ok(answer(WorkspacesOutcome::Done {}))
    }

    /// `workspaces.manager` (Ctrl+Shift+W): the window opens the workspaces' window.
    pub(crate) fn workspace_manager(&self) -> Outcome {
        self.publish(EngineEvent::WorkspaceManager);
        Outcome::Panel(self.snapshot(None))
    }

    /// The spec checked against the connections there are now, as JSON.
    async fn checked_spec(&self, spec: &WorkspaceSpec) -> Result<String, EngineError> {
        let context = self.spec_context(spec).await?;
        spec.check(&context)?;
        Ok(spec.to_json()?)
    }

    /// The connections of `spec` that exist.
    pub(super) async fn spec_context(
        &self,
        spec: &WorkspaceSpec,
    ) -> Result<SpecContext, EngineError> {
        let db = self.db()?;
        let mut context = SpecContext::default();
        for id in spec.connections() {
            if db.connection(id).await.is_ok() {
                context.connections.insert(id);
            }
        }
        Ok(context)
    }

    /// Refuses a theme this system does not offer for its mode.
    async fn check_themes(
        &self,
        light: Option<&str>,
        dark: Option<&str>,
    ) -> Result<(), EngineError> {
        if light.is_none() && dark.is_none() {
            return Ok(());
        }
        let themes = load(self.db()?).await;
        for (mode, id) in [(ThemeMode::Light, light), (ThemeMode::Dark, dark)] {
            let Some(id) = id else { continue };
            let fits = themes
                .theme(id)
                .is_some_and(|theme| theme.dark == mode.is_dark());
            if !fits {
                return Err(WorkspaceError::Theme {
                    id: id.to_owned(),
                    mode: if mode.is_dark() { "dark" } else { "light" },
                }
                .into());
            }
        }
        Ok(())
    }

    /// Refuses `name` when another workspace than `except` has it (case does not count).
    async fn require_free_name(
        &self,
        name: &str,
        except: Option<WorkspaceId>,
    ) -> Result<(), EngineError> {
        let wanted = name.to_lowercase();
        let taken = self
            .db()?
            .workspaces()
            .await?
            .iter()
            .any(|other| Some(other.id) != except && other.name.to_lowercase() == wanted);
        if taken {
            return Err(WorkspaceError::NameTaken {
                name: name.to_owned(),
            }
            .into());
        }
        Ok(())
    }

    /// Refuses a value of `workspaces.on_start` that names no workspace; empty is none.
    pub(crate) async fn check_start_workspace(&self, value: &Value) -> Result<(), EngineError> {
        let text = value.as_str().unwrap_or_default().trim();
        if text.is_empty() {
            return Ok(());
        }
        let id = text
            .parse::<WorkspaceId>()
            .map_err(|_| WorkspaceError::Unknown { id: 0 })?;
        self.db()?
            .workspace(id)
            .await?
            .ok_or(WorkspaceError::Unknown { id })?;
        Ok(())
    }

    /// Whether the connection `id` is in the connection tree.
    pub(super) async fn connection_exists(&self, id: ConnectionId) -> bool {
        match self.db() {
            Ok(db) => db.connection(id).await.is_ok(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
