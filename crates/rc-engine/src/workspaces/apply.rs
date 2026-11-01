//! `workspaces.apply` (RC-100-68): the panels show a workspace.
//!
//! First the servers the tabs are on are reached, one after the other, each as the drive bar
//! reaches it (a locked vault asked for in the window, never over MCP); a connection deleted
//! since, or a server that does not answer, leaves its tabs out with a note. Then per panel the
//! tabs, after `workspaces.apply_mode` ([`ApplyMode`]): the workspace's tabs take the place of the
//! panel's (its locked tabs stay, before them), or of all of them, or come after them; the
//! workspace's active tab is shown. Then the view (`view.rs`), the active panel and the divider.
//! Nothing that fails stops the rest: every part left out is a note in the answer, as
//! `<error code>:<what>`. The workspace is the active one from then on, and
//! [`EngineEvent::WorkspacesChanged`] says so.

use std::fmt::Display;

use rc_vfs::{BackendId, VfsPath};
use serde_json::{Value, json};

use super::{
    APPLY_MODE_KEY, ApplyMode, ON_START_KEY, PanelSpec, TabSpec, WorkspaceError, WorkspaceId,
    WorkspaceSpec, WorkspacesOutcome, spec::connection_of, store::answer,
};
use crate::{
    Engine, EngineError, EngineEvent, ListMode, Outcome, PanelId, SortOrder,
    settings::{PANEL_SPLIT_KEY, stored_setting},
    tabs::{Tab, TabId, TabSet, history::History},
};

/// One part of a workspace left out: the error code that says why, then what it was.
pub(super) fn note(code: &str, what: impl Display) -> String {
    format!("{code}:{what}")
}

/// A location in words for a note: the folder on this machine, `<scheme>://#<connection><path>`
/// on a server.
fn shown(location: &VfsPath) -> String {
    let scheme = match location.backend {
        BackendId::Sftp { .. } => "sftp",
        BackendId::Ftp { .. } => "ftp",
        _ => return location.path.display().to_string(),
    };
    let id = connection_of(location).map_or(0, |id| id.0);
    format!("{scheme}://#{id}{}", location.path.display())
}

/// A new tab on the folder of `tab`, with the panel's sort and mode where it names none.
fn new_tab(tab: &TabSpec, (sort, mode): (SortOrder, ListMode)) -> Tab {
    Tab {
        id: TabId(0),
        history: History::new(tab.location.clone()),
        path: tab.location.clone(),
        sort: tab.sort.unwrap_or(sort),
        mode: tab.mode.unwrap_or(mode),
        locked: tab.locked,
        cursor: None,
    }
}

impl Engine {
    /// `workspaces.apply`.
    pub(crate) async fn apply_workspace(&self, id: WorkspaceId) -> Result<Outcome, EngineError> {
        let _turn = self.inner.workspaces.applying.lock().await;
        let workspace = self.read_workspace(id).await?;
        let spec = workspace.spec;
        let mode = self.apply_mode().await;
        let mut skipped = Vec::new();
        let reached = self.reach_servers(&spec, &mut skipped).await;
        for panel in PanelId::BOTH {
            let panel_spec = spec.panel(panel);
            self.apply_tabs(panel, panel_spec, mode, &reached, &mut skipped)
                .await;
            self.apply_view(panel, panel_spec, &mut skipped).await;
        }
        self.switch_active(Some(spec.active));
        if let Some(split) = spec.split_permille
            && let Err(error) = self.set_setting(PANEL_SPLIT_KEY, &json!(split)).await
        {
            skipped.push(note(error.code(), PANEL_SPLIT_KEY));
        }
        self.inner.workspaces.set_active(Some(id));
        self.publish(EngineEvent::WorkspacesChanged);
        Ok(answer(WorkspacesOutcome::Applied { skipped }))
    }

    /// The workspace `workspaces.on_start` names, which the window applies at the start
    /// (`workspaces.apply`) after the last run's tabs came back - not when `RC_START_DIR` forces
    /// the start folder. `None` while the setting is empty or names no number.
    pub async fn start_workspace(&self) -> Option<WorkspaceId> {
        let value = stored_setting(self.db().ok()?, ON_START_KEY).await?;
        let text = value.as_str()?.trim();
        if text.is_empty() {
            return None;
        }
        match text.parse() {
            Ok(id) => Some(id),
            Err(_) => {
                tracing::warn!("workspaces.on_start names no workspace; none is applied");
                None
            }
        }
    }

    /// `workspaces.apply_mode`, its default without a database.
    async fn apply_mode(&self) -> ApplyMode {
        match self.db() {
            Ok(db) => stored_setting(db, APPLY_MODE_KEY)
                .await
                .as_ref()
                .and_then(Value::as_str)
                .map(ApplyMode::of_setting)
                .unwrap_or_default(),
            Err(_) => ApplyMode::default(),
        }
    }

    /// Each server the tabs of `spec` are on, reached; those that were.
    async fn reach_servers(
        &self,
        spec: &WorkspaceSpec,
        skipped: &mut Vec<String>,
    ) -> Vec<BackendId> {
        let mut servers: Vec<VfsPath> = Vec::new();
        for tab in spec.left.tabs.iter().chain(&spec.right.tabs) {
            let remote = connection_of(&tab.location).is_some();
            if remote
                && !servers
                    .iter()
                    .any(|known| known.backend == tab.location.backend)
            {
                servers.push(tab.location.clone());
            }
        }
        let announce = self.announces_locked_vault();
        let mut reached = Vec::new();
        for server in servers {
            let Some(id) = connection_of(&server) else {
                continue;
            };
            if !self.connection_exists(id).await {
                let gone: EngineError = WorkspaceError::Connection { id }.into();
                skipped.push(note(gone.code(), shown(&server)));
                continue;
            }
            let opened = self.open_vfs(&server, announce).await;
            match opened {
                Ok(_) => reached.push(server.backend),
                Err(error) => skipped.push(note(error.code(), shown(&server))),
            }
        }
        reached
    }

    /// Panel `panel` takes the tabs of `spec` after `mode`, those on a server not `reached` left
    /// out, and shows the workspace's active tab. Nothing of the workspace left: the panel stays.
    async fn apply_tabs(
        &self,
        panel: PanelId,
        spec: &PanelSpec,
        mode: ApplyMode,
        reached: &[BackendId],
        skipped: &mut Vec<String>,
    ) {
        let listing = {
            let state = self.state();
            let live = state.panel(panel);
            (live.sort, live.mode)
        };
        let mut wanted = Vec::new();
        let mut active = 0;
        for (index, tab) in spec.tabs.iter().enumerate() {
            let remote = connection_of(&tab.location).is_some();
            if remote && !reached.contains(&tab.location.backend) {
                continue;
            }
            if index == spec.active_tab {
                active = wanted.len();
            }
            wanted.push(new_tab(tab, listing));
        }
        if wanted.is_empty() {
            return;
        }
        let target = wanted[active].path.clone();
        let kept = self.kept_tabs(panel, mode);
        let active = kept.len() + active;
        let tabs: Vec<Tab> = kept.into_iter().chain(wanted).collect();
        let Some(set) = TabSet::from_tabs(tabs, active) else {
            return;
        };
        if let Err(error) = self.show_tab_set(panel, set).await {
            skipped.push(note(error.code(), shown(&target)));
        }
    }

    /// The tabs of `panel` that stay under `mode`, its live state copied into its active tab.
    fn kept_tabs(&self, panel: PanelId, mode: ApplyMode) -> Vec<Tab> {
        if mode == ApplyMode::ReplaceAll {
            return Vec::new();
        }
        let mut guard = self.state();
        let state = &mut *guard;
        let live = match panel {
            PanelId::Left => &state.left,
            PanelId::Right => &state.right,
        };
        state.tabs.sync(panel, live);
        state
            .tabs
            .set(panel)
            .tabs()
            .iter()
            .filter(|tab| mode == ApplyMode::Add || tab.locked)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
