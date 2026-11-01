//! The form on the window's right (RC-100-68): the workspace at the cursor as a draft - its
//! name, its theme of each mode, the active panel, the split and each panel (`panel_edit.rs`) -
//! saved as one `workspaces.update` ([`Form::request`]). The engine checks the spec again
//! (`WorkspaceSpec::check`); the form refuses beforehand what it can see itself, with the
//! engine's error codes: an empty or too long name, a panel without a tab.

use rc_engine::{
    PanelId, PanelSpec, UpdateWorkspaceRequest, Workspace, WorkspaceId, WorkspaceSpec,
};

use super::panel_edit::{self, PanelMsg};

/// The longest name, in characters (the engine's `MAX_NAME_CHARS`).
pub const MAX_NAME_CHARS: usize = 64;

/// The splits the form offers, in permille of the width for the left panel.
pub const SPLITS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// A change of the form.
#[derive(Debug, Clone)]
pub enum FormMsg {
    Name(String),
    /// The theme of the light or the `dark` mode, by id; `None` for the window's.
    Theme {
        dark: bool,
        id: Option<String>,
    },
    Active(PanelId),
    /// The left panel's share in permille; `None` leaves the split as it is.
    Split(Option<u16>),
    Panel(PanelId, PanelMsg),
}

/// The workspace being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub id: WorkspaceId,
    pub name: String,
    pub theme_light: Option<String>,
    pub theme_dark: Option<String>,
    pub spec: WorkspaceSpec,
    /// Changed since it was read or saved.
    pub dirty: bool,
    /// The name as stored: a save sends a name only when it changed.
    stored_name: String,
}

impl Form {
    /// The form of `workspace` as stored.
    pub fn of(workspace: &Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name.clone(),
            theme_light: workspace.theme_light.clone(),
            theme_dark: workspace.theme_dark.clone(),
            spec: workspace.spec.clone(),
            dirty: false,
            stored_name: workspace.name.clone(),
        }
    }

    /// The panel `panel` of the draft.
    pub fn panel(&self, panel: PanelId) -> &PanelSpec {
        match panel {
            PanelId::Left => &self.spec.left,
            PanelId::Right => &self.spec.right,
        }
    }

    fn panel_mut(&mut self, panel: PanelId) -> &mut PanelSpec {
        match panel {
            PanelId::Left => &mut self.spec.left,
            PanelId::Right => &mut self.spec.right,
        }
    }

    /// The theme of the light or the `dark` mode, `None` for the window's.
    pub fn theme(&self, dark: bool) -> Option<&str> {
        if dark {
            self.theme_dark.as_deref()
        } else {
            self.theme_light.as_deref()
        }
    }

    /// `message` applied; the draft is changed from now on.
    pub fn update(&mut self, message: FormMsg) {
        match message {
            FormMsg::Name(name) => self.name = name,
            FormMsg::Theme { dark: false, id } => self.theme_light = id,
            FormMsg::Theme { dark: true, id } => self.theme_dark = id,
            FormMsg::Active(panel) => self.spec.active = panel,
            FormMsg::Split(split) => self.spec.split_permille = split,
            FormMsg::Panel(panel, message) => panel_edit::apply(self.panel_mut(panel), message),
        }
        self.dirty = true;
    }

    /// The draft as one `workspaces.update`, or the error code of what the form can see is
    /// wrong: the name (`workspace_name`), a panel's tabs (`workspace_tabs`).
    pub fn request(&self) -> Result<UpdateWorkspaceRequest, &'static str> {
        let name = self.name.trim();
        if !name_fits(name) {
            return Err("workspace_name");
        }
        for panel in [&self.spec.left, &self.spec.right] {
            if panel.tabs.is_empty() || panel.active_tab >= panel.tabs.len() {
                return Err("workspace_tabs");
            }
        }
        Ok(UpdateWorkspaceRequest {
            id: self.id,
            name: (name != self.stored_name).then(|| name.to_owned()),
            spec: Some(self.spec.clone()),
            theme_light: Some(self.theme_light.clone()),
            theme_dark: Some(self.theme_dark.clone()),
        })
    }

    /// The draft is stored as it is now.
    pub fn saved(&mut self) {
        self.name = self.name.trim().to_owned();
        self.stored_name = self.name.clone();
        self.dirty = false;
    }
}

/// Whether `name` - trimmed - can name a workspace: 1 to [`MAX_NAME_CHARS`] characters, none of
/// them a control character.
pub fn name_fits(name: &str) -> bool {
    let count = name.chars().count();
    (1..=MAX_NAME_CHARS).contains(&count) && !name.chars().any(char::is_control)
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
