//! RC-100-91: F3 and F4 beside the internal associations, Alt+F3 the other viewer, Shift+F3 and
//! F3 with marked files - which files the Lister is told to step through.

use rc_vfs::VfsPath;
use serde_json::json;

use super::tests::LAUNCHED;
use super::*;
use crate::{
    Command, CursorTarget, ListerFiles,
    associations::{FileAssociation, write_rules},
    settings::{FILE_ASSOCIATIONS_KEY, STEP_THROUGH_SELECTION_KEY},
    test_support,
    user_commands::CommandRef,
};

fn launched(file: &Path) -> Option<Option<String>> {
    LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(launched, _)| launched == file)
        .map(|(_, program)| program.clone())
}

fn external(program: ExternalProgram, cursor_only: bool) -> Command {
    Command::OpenExternal {
        panel: Some(PanelId::Left),
        path: None,
        program,
        create: false,
        cursor_only,
    }
}

/// The engine with a database over `entries`, the cursor on `cursor`.
async fn engine_on(entries: &[(&str, &str)], cursor: &str) -> (tempfile::TempDir, Engine) {
    let (root, engine) = test_support::engine_with_db(entries).await;
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name(cursor.into()),
        })
        .await
        .expect("cursor");
    (root, engine)
}

/// What the window was asked to view, and which files it steps through.
async fn viewed(engine: &Engine, command: Command) -> Option<(VfsPath, ListerFiles)> {
    let mut events = engine.subscribe();
    engine.dispatch(command).await.expect("opened");
    let mut seen = None;
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::ViewFile { path, files, .. } = event {
            seen = Some((path, files));
        }
    }
    seen
}

fn viewer_path() -> &'static str {
    if cfg!(windows) {
        "C:\\Tools\\alt-viewer.exe"
    } else {
        "/opt/tools/alt-viewer"
    }
}

/// As in Total Commander, F3 and F4 never use the internal associations: a rule for the file
/// leaves the Lister and the editor alone.
#[tokio::test]
async fn f3_f4_ignore_associations() {
    let (root, engine) = engine_on(&[("app.log", "x")], "app.log").await;
    let rule = FileAssociation {
        mask: "*.log".into(),
        open: Some(CommandRef {
            command: "assoc-f3-test".into(),
            params: "%1".into(),
            ..CommandRef::default()
        }),
        ..FileAssociation::default()
    };
    engine
        .set_protected_setting(FILE_ASSOCIATIONS_KEY, &json!(write_rules(&[rule])))
        .await
        .expect("stored");
    let file = VfsPath::local(root.path()).join("app.log");
    assert_eq!(
        viewed(&engine, external(ExternalProgram::Viewer, false)).await,
        Some((file, ListerFiles::Panel))
    );
    engine
        .dispatch(external(ExternalProgram::Editor, false))
        .await
        .expect("edited");
    let editor = launched(&root.path().join("app.log")).expect("the editor got it");
    assert_ne!(editor.as_deref(), Some("assoc-f3-test"));
    let started = crate::user_commands::STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|launch| launch.program == "assoc-f3-test");
    assert!(!started, "no rule ran");
}

/// Alt+F3 is the other viewer: the system's program while F3 is the Lister, the Lister while
/// F3 is an external viewer.
#[tokio::test]
async fn alt_f3_takes_the_other_viewer() {
    let (root, engine) = engine_on(&[("a.txt", "a"), ("b.txt", "b")], "a.txt").await;
    let alt = external(ExternalProgram::AltViewer, false);
    assert_eq!(viewed(&engine, alt.clone()).await, None, "no Lister");
    assert_eq!(
        launched(&root.path().join("a.txt")),
        Some(None),
        "the system's"
    );
    engine
        .set_protected_setting(VIEWER_KEY, &json!(viewer_path()))
        .await
        .expect("stored");
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("b.txt".into()),
        })
        .await
        .expect("cursor");
    let file = VfsPath::local(root.path()).join("b.txt");
    assert_eq!(
        viewed(&engine, alt).await,
        Some((file.clone(), ListerFiles::Panel))
    );
    assert_eq!(launched(&root.path().join("b.txt")), None);
    engine
        .dispatch(external(ExternalProgram::Viewer, false))
        .await
        .expect("viewed");
    assert_eq!(
        launched(&root.path().join("b.txt")),
        Some(Some(viewer_path().to_owned()))
    );
}

/// F3 with marked files steps through them; Shift+F3 shows the file under the cursor alone; with
/// nothing marked both step through the panel, as before.
#[tokio::test]
async fn cursor_only_and_the_marked_files() {
    let (_root, engine) = engine_on(&[("a.txt", ""), ("b.txt", ""), ("c.txt", "")], "b.txt").await;
    let files = |seen: Option<(VfsPath, ListerFiles)>| seen.map(|(_, files)| files);
    let f3 = external(ExternalProgram::Viewer, false);
    let shift_f3 = external(ExternalProgram::Viewer, true);
    assert_eq!(
        files(viewed(&engine, f3.clone()).await),
        Some(ListerFiles::Panel)
    );
    assert_eq!(
        files(viewed(&engine, shift_f3.clone()).await),
        Some(ListerFiles::Panel)
    );
    {
        let mut state = engine.state();
        let selection = &mut state.panel_mut(PanelId::Left).selection;
        selection.insert("a.txt".to_owned());
        selection.insert("c.txt".to_owned());
    }
    assert_eq!(
        files(viewed(&engine, f3.clone()).await),
        Some(ListerFiles::Marked)
    );
    assert_eq!(
        files(viewed(&engine, shift_f3).await),
        Some(ListerFiles::Single)
    );
    engine
        .set_protected_setting(STEP_THROUGH_SELECTION_KEY, &json!(false))
        .await
        .expect("switched off");
    assert_eq!(files(viewed(&engine, f3).await), Some(ListerFiles::Panel));
}
