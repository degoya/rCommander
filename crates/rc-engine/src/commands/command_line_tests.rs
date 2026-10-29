use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_db::Db;
use rc_vfs::VfsPath;
use tempfile::TempDir;

use super::cmd_arguments;
use crate::test_support;
use crate::{
    Command, DriveKind, Engine, EngineError, Outcome, PanelId, drives::fake::FakeDrives,
    settings::FOLLOW_ON_OWN_DRIVE_KEY,
};

/// What `start` was handed - line, folder, keep open - by every test of the crate.
pub(super) static STARTED: Mutex<Vec<(String, PathBuf, bool)>> = Mutex::new(Vec::new());

/// The lines started in `folder`.
fn started_in(folder: &Path) -> Vec<(String, bool)> {
    STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(_, at, _)| at == folder)
        .map(|(line, _, keep)| (line.clone(), *keep))
        .collect()
}

/// `work/` (with `sub dir/inner/`) and `d/`, the root of the fake drive `D:`.
fn tree() -> TempDir {
    test_support::tree(&[("work/sub dir/inner/", ""), ("d/", "")])
}

async fn engine(root: &Path) -> Engine {
    let drives = FakeDrives::default().with("D:", DriveKind::Fixed, root.join("d"));
    Engine::open_with(
        VfsPath::local(root.join("work")),
        VfsPath::local(root.join("work")),
        Arc::new(drives),
        Duration::from_millis(100),
    )
    .await
    .expect("engine")
}

async fn run(engine: &Engine, line: &str, keep_open: bool) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::RunCommandLine {
            panel: None,
            line: line.into(),
            keep_open,
        })
        .await
}

fn path(engine: &Engine) -> PathBuf {
    engine.panel(PanelId::Left).path.path
}

#[tokio::test]
async fn cd_moves_the_active_panel_and_starts_nothing() {
    let root = tree();
    let engine = engine(root.path()).await;
    let work = root.path().join("work");

    run(&engine, "cd \"sub dir\"", false).await.expect("cd");
    assert_eq!(path(&engine), work.join("sub dir"));
    run(&engine, "cd inner", false).await.expect("relative");
    assert_eq!(path(&engine), work.join("sub dir").join("inner"));
    run(&engine, "cd..", false).await.expect("up");
    assert_eq!(path(&engine), work.join("sub dir"));
    let absolute = format!("cd {}", work.join("sub dir").join("inner").display());
    run(&engine, &absolute, false).await.expect("absolute");
    assert_eq!(path(&engine), work.join("sub dir").join("inner"));
    run(&engine, "cd", false).await.expect("stays");
    assert_eq!(path(&engine), work.join("sub dir").join("inner"));
    assert_eq!(
        engine.panel(PanelId::Right).path.path,
        work,
        "only the active panel"
    );

    for folder in [
        &work,
        &work.join("sub dir"),
        &work.join("sub dir").join("inner"),
    ] {
        assert!(
            started_in(folder).is_empty(),
            "nothing started in {folder:?}"
        );
    }
}

#[tokio::test]
async fn a_missing_folder_fails_and_the_panel_stays() {
    let root = tree();
    let engine = engine(root.path()).await;
    let error = run(&engine, "cd nowhere", false)
        .await
        .expect_err("missing");
    assert_eq!(error.code(), "not_found");
    assert_eq!(path(&engine), root.path().join("work"));
}

#[tokio::test]
async fn a_drive_changes_the_drive_without_a_process() {
    let root = tree();
    let engine = engine(root.path()).await;
    run(&engine, "d:", false).await.expect("drive");
    assert_eq!(path(&engine), root.path().join("d"));
    run(&engine, "cd /d d:", false).await.expect("drive again");
    assert_eq!(path(&engine), root.path().join("d"));
    assert!(started_in(&root.path().join("work")).is_empty());
}

/// `d:` with the other panel on `D:` goes to the other panel's folder, as in Total Commander.
#[tokio::test]
async fn a_drive_the_other_panel_shows_opens_at_its_folder() {
    let root = tree();
    let folder = root.path().join("d").join("test");
    fs::create_dir(&folder).expect("mkdir");
    let engine = engine(root.path()).await;
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: folder.to_string_lossy().into_owned(),
        })
        .await
        .expect("navigates");
    run(&engine, "d:", false).await.expect("drive");
    assert_eq!(path(&engine), folder);
}

async fn go(engine: &Engine, panel: PanelId, to: &Path) {
    let to = Command::Navigate {
        panel: Some(panel),
        path: to.to_string_lossy().into_owned(),
    };
    engine.dispatch(to).await.expect("navigates");
}

/// `d:` with both panels on `D:` goes to the other panel's folder too (owner 2026-10-26,
/// RC-010-29); a change by the drive bar keeps its own drive, and so does `d:` with
/// `drives.follow_on_own_drive` off.
#[tokio::test]
async fn a_drive_the_panel_is_on_follows_the_other_panel_too() {
    let root = tree();
    let d = root.path().join("d");
    let folder = d.join("test");
    fs::create_dir(&folder).expect("mkdir");
    let engine = engine(root.path()).await;
    go(&engine, PanelId::Right, &folder).await;
    go(&engine, PanelId::Left, &d).await;

    let bar = Command::ChangeDrive {
        panel: Some(PanelId::Left),
        drive: "D:".into(),
    };
    engine.dispatch(bar).await.expect("changes");
    assert_eq!(
        path(&engine),
        d,
        "the drive bar keeps the panel's own drive"
    );
    run(&engine, "d:", false).await.expect("drive");
    assert_eq!(path(&engine), folder);

    go(&engine, PanelId::Left, &d).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let off = Command::SetSetting {
        key: FOLLOW_ON_OWN_DRIVE_KEY.into(),
        value: serde_json::json!(false),
    };
    engine.dispatch(off).await.expect("stored");
    run(&engine, "d:", false).await.expect("drive");
    assert_eq!(path(&engine), d, "off: the rule of 0.7.0");
}

#[tokio::test]
async fn any_other_line_runs_in_the_panels_folder() {
    let root = tree();
    let engine = engine(root.path()).await;
    let work = root.path().join("work");
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "sub dir".into(),
        })
        .await
        .expect("right panel moves");

    run(&engine, "  dir /w ", false).await.expect("runs");
    engine
        .dispatch(Command::RunCommandLine {
            panel: Some(PanelId::Right),
            line: "ls -la".into(),
            keep_open: true,
        })
        .await
        .expect("runs");

    assert_eq!(started_in(&work), [("dir /w".to_owned(), false)]);
    assert_eq!(
        started_in(&work.join("sub dir")),
        [("ls -la".to_owned(), true)]
    );
    assert_eq!(path(&engine), work, "the panel stays");
}

#[tokio::test]
async fn a_bad_line_is_refused_before_anything_runs() {
    let root = tree();
    let engine = engine(root.path()).await;
    for line in ["", "   ", "dir\r\ndel *"] {
        let error = run(&engine, line, false).await.expect_err("refused");
        assert_eq!(error.code(), "command_line_invalid", "{line:?}");
    }
    assert!(started_in(&root.path().join("work")).is_empty());
}

#[test]
fn cmd_gets_the_line_as_typed_inside_its_own_quotes() {
    assert_eq!(
        cmd_arguments("copy \"a b.txt\" c", false),
        "/s /c \"copy \"a b.txt\" c\""
    );
    assert_eq!(cmd_arguments("dir", true), "/s /k \"dir\"");
}

/// The real shell, once: the line runs in the folder it is given.
#[cfg(unix)]
#[tokio::test]
async fn sh_runs_the_line_in_the_folder() {
    let folder = tempfile::tempdir().expect("temp dir");
    let mut command = super::shell_command("pwd > where.txt", false);
    command.current_dir(folder.path());
    let status = tokio::process::Command::from(command)
        .status()
        .await
        .expect("sh runs");
    assert!(status.success());
    let written = fs::read_to_string(folder.path().join("where.txt")).expect("written");
    let expected = folder.path().canonicalize().expect("canonical");
    assert_eq!(
        Path::new(written.trim()).canonicalize().expect("canonical"),
        expected
    );
}

#[cfg(windows)]
#[test]
fn cmd_comes_from_the_system_folder() {
    let command = super::shell_command("dir", false);
    let program = PathBuf::from(command.get_program());
    assert!(program.ends_with("System32\\cmd.exe"), "{program:?}");
}
