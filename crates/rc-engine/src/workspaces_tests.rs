use super::*;
use crate::{Engine, VfsPath};

fn workspace(id: WorkspaceId, active: bool, light: Option<&str>, dark: Option<&str>) -> Workspace {
    Workspace {
        id,
        name: format!("w{id}"),
        spec: spec::sample(),
        theme_light: light.map(str::to_owned),
        theme_dark: dark.map(str::to_owned),
        active,
    }
}

/// The active workspace's theme takes the place of the window's for its mode; a mode it leaves
/// open, or no active workspace, leaves the window's.
#[test]
fn the_active_workspace_gives_its_theme_per_mode() {
    let list = [
        workspace(1, false, Some("classic-light"), Some("nord-dark")),
        workspace(2, true, None, Some("solarized-dark")),
    ];
    assert_eq!(theme_of(&list, ThemeMode::Dark), Some("solarized-dark"));
    assert_eq!(theme_of(&list, ThemeMode::Light), None, "the window's");
    let none_active = [workspace(1, false, Some("classic-light"), None)];
    assert_eq!(theme_of(&none_active, ThemeMode::Light), None);
    assert_eq!(theme_of(&[], ThemeMode::Dark), None);
}

#[tokio::test]
async fn no_workspace_is_active_at_first() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert_eq!(engine.active_workspace(), None);
    engine.inner.workspaces.set_active(Some(4));
    assert_eq!(engine.active_workspace(), Some(4));
}
