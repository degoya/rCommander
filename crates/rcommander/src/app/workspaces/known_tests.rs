use super::*;
use crate::app::workspaces::fixtures::workspace;

/// The rows keep the list's order, names and the active mark.
#[test]
fn the_rows_follow_the_list() {
    let list = vec![workspace(4, "Servers", false), workspace(2, "Web", true)];
    let rows = rows_of(&list);
    assert_eq!(
        rows,
        [
            WorkspaceRow {
                id: 4,
                name: "Servers".to_owned(),
                active: false,
            },
            WorkspaceRow {
                id: 2,
                name: "Web".to_owned(),
                active: true,
            },
        ]
    );
}

/// Only the active workspace's themes count, each mode on its own; none while none is active.
#[test]
fn the_active_workspace_gives_its_themes() {
    let mut active = workspace(2, "Web", true);
    active.theme_dark = Some("nord-dark".to_owned());
    let mut other = workspace(4, "Servers", false);
    other.theme_light = Some("classic-light".to_owned());
    assert_eq!(
        active_themes(&[other.clone(), active]),
        [None, Some("nord-dark".to_owned())]
    );
    assert_eq!(active_themes(&[other]), [None, None]);
    assert_eq!(active_themes(&[]), [None, None]);
}
