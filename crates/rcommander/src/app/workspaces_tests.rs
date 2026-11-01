use rc_engine::PanelId;

use super::*;
use crate::app::workspaces::fixtures::workspace;

fn list() -> Vec<Workspace> {
    vec![
        workspace(5, "Servers", false),
        workspace(2, "Local and server", true),
        workspace(9, "Web", false),
    ]
}

/// The window opens on the active workspace with its form; the form follows the cursor.
#[test]
fn the_window_opens_on_the_active_workspace() {
    let mut window = WorkspaceWindow::default();
    window.refresh(&list());
    assert_eq!(window.cursor, Some(2));
    assert_eq!(window.form.as_ref().map(|form| form.id), Some(2));
    window.select(Some(9), &list());
    assert_eq!(
        window.form.as_ref().map(|form| form.name.as_str()),
        Some("Web")
    );
    window.select(Some(4), &list());
    assert!(window.form.is_none(), "no such workspace");
}

/// A new list keeps the cursor and a draft with changes; a draft without changes, or another
/// workspace's, is read anew.
#[test]
fn a_new_list_keeps_the_draft() {
    let mut window = WorkspaceWindow::default();
    window.refresh(&list());
    if let Some(form) = window.form.as_mut() {
        form.update(FormMsg::Active(PanelId::Right));
    }
    let mut renamed = list();
    renamed[1].name = "Renamed".to_owned();
    window.refresh(&renamed);
    let form = window.form.as_ref().expect("the draft");
    assert!(form.dirty);
    assert_eq!(form.spec.active, PanelId::Right, "as typed");

    window.form = window.form.take().map(|mut form| {
        form.saved();
        form
    });
    window.refresh(&renamed);
    let form = window.form.as_ref().expect("read anew");
    assert_eq!(form.name, "Renamed");
    assert_eq!(form.spec.active, PanelId::Left);
}

/// A workspace deleted from under the cursor gives the cursor to the active one; the filter
/// keeps it among the workspaces shown.
#[test]
fn the_cursor_stays_among_the_shown() {
    let mut window = WorkspaceWindow::default();
    window.refresh(&list());
    window.select(Some(9), &list());
    let without: Vec<Workspace> = list().into_iter().filter(|w| w.id != 9).collect();
    window.refresh(&without);
    assert_eq!(window.cursor, Some(2));
    window.filter = "serv".to_owned();
    window.select(Some(5), &without);
    window.refresh(&without);
    assert_eq!(window.cursor, Some(5));
    window.filter = "nothing".to_owned();
    window.refresh(&without);
    assert_eq!(window.cursor, None);
    assert!(window.form.is_none());
}
