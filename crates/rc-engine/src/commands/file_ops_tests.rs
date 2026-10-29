use std::{
    fs,
    path::{MAIN_SEPARATOR, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rc_ops::{JobOptions, JobState};
use rc_vfs::VfsPath;

use super::*;
use crate::test_support;
use crate::test_support::expect_jobs;
use crate::{Command, settings::is_protected};

/// What `launch` was handed, by every test of this module: the file and the program.
pub(super) static LAUNCHED: Mutex<Vec<(PathBuf, Option<String>)>> = Mutex::new(Vec::new());

fn launched(file: &Path) -> Option<Option<String>> {
    LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(launched, _)| launched == file)
        .map(|(_, program)| program.clone())
}

async fn engine() -> (tempfile::TempDir, Engine) {
    let root = test_support::tree(&[("docs/", ""), ("out/", ""), ("a.txt", "a"), ("b.txt", "b")]);
    let engine = test_support::open(root.path(), &root.path().join("out")).await;
    (root, engine)
}

/// Runs `command`, waits for its jobs - one per source, RC-010-14 - and answers their final
/// state: the first that did not complete, else `Completed`.
async fn run_job(engine: &Engine, command: Command) -> JobState {
    let accepted = engine.dispatch(command).await.expect("accepted");
    let jobs: Vec<rc_ops::JobId> = expect_jobs(accepted).iter().map(|job| job.id).collect();
    let mut state = JobState::Completed;
    for job in jobs {
        let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job))
            .await
            .expect("in time")
            .expect("known")
            .state;
        if state == JobState::Completed {
            state = done;
        }
    }
    state
}

fn copy_to(sources: &[&str], target: &str) -> Command {
    Command::OpsCopyTo {
        panel: Some(PanelId::Left),
        sources: sources.iter().map(|name| (*name).to_owned()).collect(),
        target: target.to_owned(),
        options: JobOptions::default(),
        queue: None,
    }
}

#[test]
fn a_typed_target_names_a_directory_by_a_mask_or_a_trailing_separator() {
    let sep = MAIN_SEPARATOR;
    assert_eq!(typed_target(" copy.txt "), ("copy.txt", false));
    assert_eq!(
        typed_target(&format!("out{sep}")),
        (&*format!("out{sep}"), true)
    );
    assert_eq!(
        typed_target(&format!("out{sep}*.*")),
        (&*format!("out{sep}"), true)
    );
    assert_eq!(
        typed_target(&format!("out{sep}*")),
        (&*format!("out{sep}"), true)
    );
    assert_eq!(typed_target("*.*"), ("", true));
    assert_eq!(
        typed_target("report*"),
        ("report*", false),
        "a star inside a name is no mask"
    );
}

/// The acceptance check of RC-010-07: a relative target is resolved against the source
/// directory, not the other panel's.
#[tokio::test]
async fn a_relative_target_is_resolved_against_the_source_directory() {
    let (root, engine) = engine().await;
    let state = run_job(&engine, copy_to(&["a.txt"], "copy.txt")).await;
    assert_eq!(state, JobState::Completed);
    assert_eq!(
        fs::read(root.path().join("copy.txt")).expect("copied"),
        b"a"
    );
    assert!(!root.path().join("out").join("copy.txt").exists());
    assert!(
        root.path().join("a.txt").exists(),
        "a copy keeps its source"
    );
}

#[tokio::test]
async fn an_existing_directory_or_a_trailing_separator_is_copied_into() {
    let (root, engine) = engine().await;
    assert_eq!(
        run_job(&engine, copy_to(&["a.txt"], "docs")).await,
        JobState::Completed
    );
    assert!(root.path().join("docs").join("a.txt").exists());
    let new_dir = format!("fresh{MAIN_SEPARATOR}");
    assert_eq!(
        run_job(&engine, copy_to(&["b.txt"], &new_dir)).await,
        JobState::Completed
    );
    assert!(root.path().join("fresh").join("b.txt").exists());
    let absolute = root.path().join("out").display().to_string();
    assert_eq!(
        run_job(&engine, copy_to(&["a.txt", "b.txt"], &absolute)).await,
        JobState::Completed
    );
    assert!(root.path().join("out").join("a.txt").exists());
    assert!(root.path().join("out").join("b.txt").exists());
}

/// Whether the target is a directory to copy into is decided once, for the whole selection,
/// before it is split into one job per source (RC-010-14): every job copies into `both`, also
/// the second, which finds the folder the first one made.
#[tokio::test]
async fn several_sources_go_into_a_new_directory() {
    let (root, engine) = engine().await;
    let outcome = engine
        .dispatch(copy_to(&["a.txt", "b.txt"], "both"))
        .await
        .expect("accepted");
    let Outcome::Jobs { jobs } = outcome else {
        panic!("one job per source, got {outcome:?}");
    };
    let into = root.path().join("both");
    for job in &jobs {
        assert!(
            matches!(&job.kind, rc_ops::JobKind::Copy { target, .. } if target.path == into),
            "{job:?}"
        );
        let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
            .await
            .expect("in time")
            .expect("known");
        assert_eq!(done.state, JobState::Completed);
    }
    assert!(into.join("a.txt").is_file() && into.join("b.txt").is_file());
    fs::remove_dir_all(&into).expect("cleared");
    assert_eq!(
        run_job(&engine, copy_to(&["a.txt", "b.txt"], "both")).await,
        JobState::Completed
    );
    assert!(root.path().join("both").join("a.txt").exists());
    assert!(root.path().join("both").join("b.txt").exists());
}

#[tokio::test]
async fn move_to_a_new_name_renames() {
    let (root, engine) = engine().await;
    let rename = Command::OpsMoveTo {
        panel: Some(PanelId::Left),
        sources: vec!["a.txt".into()],
        target: "renamed.txt".into(),
        options: JobOptions::default(),
        queue: None,
    };
    assert_eq!(run_job(&engine, rename).await, JobState::Completed);
    assert!(!root.path().join("a.txt").exists());
    assert_eq!(
        fs::read(root.path().join("renamed.txt")).expect("moved"),
        b"a"
    );
}

#[tokio::test]
async fn copy_onto_itself_and_an_empty_target_are_refused() {
    let (_root, engine) = engine().await;
    let same = engine.dispatch(copy_to(&["a.txt"], "a.txt")).await;
    assert_eq!(same.map(|_| ()).map_err(|e| e.code()), Err("same_location"));
    let empty = engine.dispatch(copy_to(&["a.txt"], "  ")).await;
    assert_eq!(empty.map(|_| ()).map_err(|e| e.code()), Err("invalid_name"));
}

#[tokio::test]
async fn the_dialog_command_is_announced_for_the_window() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::FileDialog {
            panel: Some(PanelId::Right),
            dialog: FileDialog::Mkdir,
        })
        .await
        .expect("dispatched");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::FileDialog {
            panel: PanelId::Right,
            dialog: FileDialog::Mkdir,
        })
    );
}

/// Without an external viewer F3 is the internal viewer: the window is asked for its Lister, as
/// by `view.open`, and no program starts.
#[tokio::test]
async fn f3_without_an_external_viewer_opens_the_internal_viewer() {
    let (root, engine) = engine().await;
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: crate::CursorTarget::Name("a.txt".into()),
        })
        .await
        .expect("cursor");
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::OpenExternal {
            panel: None,
            path: None,
            program: ExternalProgram::Viewer,
            create: false,
        })
        .await
        .expect("opened");
    let file = root.path().join("a.txt");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::ViewFile {
            panel: PanelId::Left,
            path: VfsPath::local(root.path()).join("a.txt"),
        })
    );
    assert_eq!(launched(&file), None);
}

#[tokio::test]
async fn shift_f4_creates_a_missing_file_and_never_a_bad_name() {
    let (root, engine) = engine().await;
    let open = |path: &str| Command::OpenExternal {
        panel: Some(PanelId::Left),
        path: Some(path.into()),
        program: ExternalProgram::Editor,
        create: true,
    };
    engine.dispatch(open("notes.txt")).await.expect("created");
    let made = root.path().join("notes.txt");
    assert_eq!(fs::read(&made).expect("made"), b"");
    assert!(launched(&made).is_some());
    let refused = engine.dispatch(open("con.txt")).await;
    assert_eq!(
        refused.map(|_| ()).map_err(|e| e.code()),
        Err("invalid_name")
    );
    let directory = engine.dispatch(open("docs")).await;
    assert_eq!(
        directory.map(|_| ()).map_err(|e| e.code()),
        Err("is_a_directory")
    );
}

#[test]
fn only_a_person_picks_the_viewer_and_the_editor() {
    assert!(is_protected(VIEWER_KEY));
    assert!(is_protected(EDITOR_KEY));
}

/// TC's "use external viewer": a viewer set in the settings gets the file.
#[tokio::test]
async fn f3_with_an_external_viewer_set_hands_the_file_to_it() {
    let (root, engine) = engine().await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    let viewer = if cfg!(windows) {
        "C:\\Tools\\viewer.exe"
    } else {
        "/opt/tools/viewer"
    };
    engine
        .set_protected_setting(VIEWER_KEY, &serde_json::json!(viewer))
        .await
        .expect("stored");
    engine
        .dispatch(Command::OpenExternal {
            panel: Some(PanelId::Left),
            path: Some("b.txt".into()),
            program: ExternalProgram::Viewer,
            create: false,
        })
        .await
        .expect("opened");
    assert_eq!(
        launched(&root.path().join("b.txt")),
        Some(Some(viewer.to_owned()))
    );
}

/// A folder holding the archive fixture `name`, shown open in the left panel.
async fn inside_archive(name: &str) -> (tempfile::TempDir, Engine) {
    let (root, engine) = crate::archives::tests::engine_beside(name).await;
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: crate::CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
    engine
        .dispatch(Command::OpenFile {
            panel: None,
            path: None,
        })
        .await
        .expect("opened");
    assert!(matches!(
        engine.panel(PanelId::Left).path.backend,
        BackendId::Archive { .. }
    ));
    (root, engine)
}

/// RC-030-02: F3 on a text file inside a 7z opens the internal viewer on the entry itself, and
/// the viewer reads it through the archive; no program starts.
#[tokio::test]
async fn f3_inside_an_archive_views_the_entry() {
    let (_root, engine) = inside_archive("tree.7z").await;
    let entry = engine
        .panel(PanelId::Left)
        .path
        .join("docs")
        .join("readme.md");
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::OpenExternal {
            panel: None,
            path: Some("docs/readme.md".into()),
            program: ExternalProgram::Viewer,
            create: false,
        })
        .await
        .expect("opened");
    let mut viewed = None;
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::ViewFile { panel, path } = event {
            viewed = Some((panel, path));
        }
    }
    assert_eq!(viewed, Some((PanelId::Left, entry.clone())));
    let window = engine.open_for_view(&entry).await.expect("opens");
    let text = b"# Readme\n\nInside an archive.\n";
    assert_eq!(window.size(), u64::try_from(text.len()).expect("small"));
}

/// RC-030-02: F4 on a file inside an archive hands the editor a read-only copy under the
/// temporary folder; the archive is not written.
#[tokio::test]
async fn f4_inside_an_archive_hands_the_editor_a_temporary_copy() {
    let (root, engine) = inside_archive("tree.zip").await;
    engine
        .dispatch(Command::OpenExternal {
            panel: None,
            path: Some("hello.txt".into()),
            program: ExternalProgram::Editor,
            create: false,
        })
        .await
        .expect("opened");
    let copy = LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(|(file, _)| file.clone())
        .find(|file| {
            file.file_name() == Some("hello.txt".as_ref()) && !file.starts_with(root.path())
        })
        .expect("a copy was handed on");
    assert_eq!(fs::read(&copy).expect("read"), b"hello archive\n");
    assert!(fs::metadata(&copy).expect("stat").permissions().readonly());
    if let Some(dir) = copy.parent() {
        // Best effort: Windows keeps a read-only file.
        let _ = fs::remove_dir_all(dir);
    }
}
