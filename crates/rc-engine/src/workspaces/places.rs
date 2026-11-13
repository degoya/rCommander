//! A workspace's places in words (RC-110-21): what applying it opens. The MCP server judges the
//! folders of this machine a spec names before it is saved ([`WorkspaceSpec::local_places`]: a
//! share on another machine is refused there, RC-080-10) and shows the tabs and the terminal of
//! each panel in the question about `workspaces_apply` ([`PanelSpec::shown_tabs`],
//! [`PanelSpec::shown_terminal`]), so the person reads where the panels go, not only an id.

use rc_vfs::BackendId;

use super::{PanelSpec, TermSpec, WorkspaceSpec, apply::shown};
use crate::PanelView;

impl WorkspaceSpec {
    /// Every folder the spec names that this machine opens: the tabs on this machine and each
    /// terminal's start folder, whatever view its panel has - a share among them is one Windows
    /// would sign in to. A tab on a server is the server's path and not among them.
    #[must_use]
    pub fn local_places(&self) -> Vec<String> {
        let mut found = Vec::new();
        for panel in [&self.left, &self.right] {
            for tab in &panel.tabs {
                if tab.location.backend == BackendId::Local {
                    found.push(tab.location.path.to_string_lossy().into_owned());
                }
            }
            if let Some(cwd) = panel.terminal.as_ref().and_then(TermSpec::cwd) {
                found.push(cwd.to_owned());
            }
        }
        found
    }
}

impl PanelSpec {
    /// The panel's tabs, left to right: the folder on this machine,
    /// `<scheme>://#<connection><path>` on a server.
    #[must_use]
    pub fn shown_tabs(&self) -> Vec<String> {
        self.tabs.iter().map(|tab| shown(&tab.location)).collect()
    }

    /// The terminal applying the workspace shows in this panel - its kind, what it runs on and
    /// its start folder; `None` for a panel that shows its file list.
    #[must_use]
    pub fn shown_terminal(&self) -> Option<String> {
        let terminal = self
            .terminal
            .as_ref()
            .filter(|_| self.view == PanelView::Terminal)?;
        Some(match terminal.cwd() {
            Some(cwd) => format!("{} in {cwd}", terminal.describe()),
            None => terminal.describe(),
        })
    }
}

impl TermSpec {
    /// The folder the terminal starts in, where the spec names one.
    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        match self {
            Self::Local { cwd, .. } | Self::Wsl { cwd, .. } | Self::Ssh { cwd, .. } => {
                cwd.as_deref()
            }
        }
    }
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
