use rc_engine::{ConnectionId, PanelView, TermSpec};
use rc_vfs::VfsPath;

use super::*;
use crate::app::workspaces::fixtures::workspace;

/// A form as read changes nothing; every edit makes it a draft to save.
#[test]
fn an_edit_makes_a_draft() {
    let stored = workspace(3, "Servers", false);
    let mut form = Form::of(&stored);
    assert!(!form.dirty);
    assert_eq!(form.spec, stored.spec);
    form.update(FormMsg::Active(PanelId::Right));
    assert!(form.dirty);
    assert_eq!(form.spec.active, PanelId::Right);
}

/// The form writes a valid spec: the edited panels, the active panel, the split and the themes
/// in one `workspaces.update`; the name only when it changed.
#[test]
fn the_form_writes_its_spec() {
    let mut form = Form::of(&workspace(3, "Servers", false));
    form.update(FormMsg::Split(Some(400)));
    form.update(FormMsg::Theme {
        dark: true,
        id: Some("nord-dark".to_owned()),
    });
    form.update(FormMsg::Panel(
        PanelId::Right,
        PanelMsg::AddTab(VfsPath::local("/srv")),
    ));
    form.update(FormMsg::Panel(PanelId::Right, PanelMsg::ActiveTab(1)));
    form.update(FormMsg::Panel(
        PanelId::Left,
        PanelMsg::View(PanelView::Terminal),
    ));
    form.update(FormMsg::Panel(
        PanelId::Left,
        PanelMsg::Terminal(TermSpec::Ssh {
            connection_id: ConnectionId(4),
            cwd: None,
        }),
    ));
    let request = form.request().expect("a valid form");
    assert_eq!(request.id, 3);
    assert_eq!(request.name, None, "the name did not change");
    assert_eq!(request.theme_light, Some(None));
    assert_eq!(request.theme_dark, Some(Some("nord-dark".to_owned())));
    let spec = request.spec.expect("the spec");
    assert_eq!(spec.version, 1, "the version read stays");
    assert_eq!(spec.split_permille, Some(400));
    assert_eq!(spec.right.tabs.len(), 2);
    assert_eq!(spec.right.active_tab, 1);
    assert_eq!(spec.left.view, PanelView::Terminal);
    assert!(matches!(spec.left.terminal, Some(TermSpec::Ssh { .. })));

    form.update(FormMsg::Name("  Build servers ".to_owned()));
    let request = form.request().expect("a valid form");
    assert_eq!(request.name.as_deref(), Some("Build servers"), "trimmed");
    form.saved();
    assert!(!form.dirty);
    assert_eq!(form.request().expect("valid").name, None, "stored now");
}

/// What the form sees is wrong is refused with the engine's error codes.
#[test]
fn the_form_refuses_what_it_sees() {
    let mut form = Form::of(&workspace(3, "Servers", false));
    for name in ["", "   ", "tab\there"] {
        form.update(FormMsg::Name(name.to_owned()));
        assert_eq!(form.request().err(), Some("workspace_name"), "{name:?}");
    }
    form.update(FormMsg::Name("x".repeat(MAX_NAME_CHARS + 1)));
    assert_eq!(form.request().err(), Some("workspace_name"));
    form.update(FormMsg::Name("\u{0100}".repeat(MAX_NAME_CHARS)));
    assert!(form.request().is_ok(), "characters, not bytes");
    form.spec.left.tabs.clear();
    assert_eq!(form.request().err(), Some("workspace_tabs"));
}
