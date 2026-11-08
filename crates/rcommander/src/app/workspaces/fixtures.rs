//! Workspaces for the tests of the window, its palette rows and its menu (RC-100-68), built as
//! the contract with the engine spells them.

use rc_engine::{PanelId, PanelSpec, PanelView, TabSpec, Workspace, WorkspaceId, WorkspaceSpec};
use rc_vfs::VfsPath;

/// A tab on this machine at `path`.
pub fn tab(path: &str) -> TabSpec {
    TabSpec {
        location: VfsPath::local(path),
        sort: None,
        mode: None,
        locked: false,
        title: None,
    }
}

/// A panel with a tab at each of `paths`, the first active, showing its files.
pub fn panel(paths: &[&str]) -> PanelSpec {
    PanelSpec {
        tabs: paths.iter().map(|path| tab(path)).collect(),
        active_tab: 0,
        view: PanelView::Files,
        terminal: None,
    }
}

/// Two panels of one tab each, the left active, the split left alone.
pub fn spec() -> WorkspaceSpec {
    WorkspaceSpec {
        version: 1,
        active: PanelId::Left,
        split_permille: None,
        left: panel(&["/left"]),
        right: panel(&["/right"]),
    }
}

/// The workspace `id` named `name` with [`spec`], without themes.
pub fn workspace(id: WorkspaceId, name: &str, active: bool) -> Workspace {
    Workspace {
        id,
        name: name.to_owned(),
        spec: spec(),
        theme_light: None,
        theme_dark: None,
        active,
    }
}
