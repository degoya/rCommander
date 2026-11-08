use std::path::{Path, PathBuf};

use serde_json::json;

use super::OpenRemote;
use crate::recover::LockOrRecover as _;
use crate::remote_edit::{
    MAX_REMOTE_EDITS, RemoteEditState,
    folder::{self, owner},
    launch::LAUNCHED,
    test_support::Rig,
};
use crate::{
    Command, EngineError, ExternalProgram, Outcome, PanelId,
    read_guard::as_mcp,
    settings::{EDITOR_KEY, REMOTE_EDIT_PROGRAMS_KEY},
};

/// The program `file` was handed to, if it was.
fn launched(file: &Path) -> Option<Option<String>> {
    LAUNCHED
        .lock_or_recover("test.launched")
        .iter()
        .find(|(launched, _)| launched == file)
        .map(|(_, program)| program.clone())
}

/// A program path the settings take as absolute on every platform.
fn program(name: &str) -> String {
    if cfg!(windows) {
        format!("C:\\Programs\\{name}.exe")
    } else {
        format!("/usr/bin/{name}")
    }
}

fn editor(program: &str) -> OpenRemote {
    OpenRemote {
        program: Some(program.into()),
        ..OpenRemote::default()
    }
}

/// F4 on a server file in the panel: fetched as a job into `edit-<pid>-*`, handed to the editor
/// of the settings, watched.
#[tokio::test]
async fn f4_fetches_the_file_and_starts_the_editor_on_the_copy() {
    let rig = Rig::new(&[("/etc/app.conf", "port=1")]).await;
    rig.engine
        .set_protected_setting(EDITOR_KEY, &json!(program("gedit")))
        .await
        .expect("editor");
    rig.engine
        .show_path(PanelId::Left, Rig::at("/etc"))
        .await
        .expect("the server's folder");
    let mut jobs = rig.engine.subscribe();
    let outcome = rig
        .engine
        .dispatch(Command::OpenExternal {
            panel: Some(PanelId::Left),
            path: Some("app.conf".into()),
            program: ExternalProgram::Editor,
            create: false,
            cursor_only: false,
        })
        .await
        .expect("opened");
    assert!(matches!(outcome, Outcome::Panel(_)));

    let edit = rig.edit("app.conf");
    assert_eq!(edit.state, RemoteEditState::Unchanged);
    assert_eq!(edit.path, Rig::at("/etc/app.conf"));
    let copy = rig.copy(edit.id);
    assert_eq!(std::fs::read_to_string(&copy).expect("copy"), "port=1");
    let dir = copy.parent().expect("its folder");
    assert_eq!(dir.parent(), Some(rig.edits.path()));
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    assert_eq!(owner(name), Some(std::process::id()));
    assert!(folder::session_path(dir).exists(), "its session beside it");
    assert_eq!(launched(&copy), Some(Some(program("gedit"))));
    // It came through the queue.
    let mut job_seen = false;
    while let Ok(event) = jobs.try_recv() {
        job_seen |= matches!(event, crate::EngineEvent::Job { .. });
    }
    assert!(job_seen, "the fetch is a job of the queue");
}

/// Enter (the system's program), F3 with an external viewer and Shift+F4 (a new empty file)
/// open a watched copy the same way.
#[tokio::test]
async fn enter_f3_and_shift_f4_open_a_watched_copy_too() {
    let rig = Rig::new(&[("/etc/readme.txt", "hi"), ("/etc/view.log", "log")]).await;
    rig.engine
        .show_path(PanelId::Left, Rig::at("/etc"))
        .await
        .expect("the server's folder");
    rig.engine
        .dispatch(Command::OpenFile {
            panel: Some(PanelId::Left),
            path: Some("readme.txt".into()),
        })
        .await
        .expect("enter");
    let copy = rig.copy(rig.edit("readme.txt").id);
    assert_eq!(launched(&copy), Some(None), "the system's program");

    rig.engine
        .set_protected_setting(crate::settings::VIEWER_KEY, &json!(program("less")))
        .await
        .expect("viewer");
    rig.engine
        .dispatch(Command::OpenExternal {
            panel: Some(PanelId::Left),
            path: Some("view.log".into()),
            program: ExternalProgram::Viewer,
            create: false,
            cursor_only: false,
        })
        .await
        .expect("f3");
    let viewed = rig.copy(rig.edit("view.log").id);
    assert_eq!(launched(&viewed), Some(Some(program("less"))));

    rig.engine
        .dispatch(Command::OpenExternal {
            panel: Some(PanelId::Left),
            path: Some("new.txt".into()),
            program: ExternalProgram::Editor,
            create: true,
            cursor_only: false,
        })
        .await
        .expect("shift+f4");
    assert_eq!(
        rig.server.text("/etc/new.txt").await,
        "",
        "made on the server"
    );
    let fresh = rig.copy(rig.edit("new.txt").id);
    assert_eq!(std::fs::read(&fresh).expect("copy"), b"");
    assert_eq!(rig.list().len(), 3);
}

/// A name a local folder does not take is not fetched (RC-080-43).
#[tokio::test]
async fn a_name_a_local_folder_refuses_is_not_fetched() {
    // `a:b` is refused before the server is asked: the memory server on Windows takes no such
    // name, so only `con` is put there.
    let rig = Rig::new(&[("/etc/con", "x")]).await;
    for name in ["con", "a:b"] {
        let refused = rig
            .engine
            .edit_remote(Rig::at(&format!("/etc/{name}")), editor("vi"))
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), "invalid_name", "{name}");
    }
    assert!(rig.list().is_empty());
    let left: Vec<PathBuf> = std::fs::read_dir(rig.edits.path())
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "no folder made: {left:?}");
}

/// Enter on a file that would run as a program is refused, unless the setting allows it.
#[tokio::test]
async fn enter_on_a_program_is_refused_unless_allowed() {
    let rig = Rig::new(&[("/etc/tool.exe", "MZ"), ("/etc/deploy", "#!/bin/sh")]).await;
    rig.server.set_mode("/etc/deploy", 0o755);
    rig.server.set_mode("/etc/tool.exe", 0o755);
    let enter = || OpenRemote {
        system: true,
        ..OpenRemote::default()
    };
    let refused = rig
        .engine
        .edit_remote(Rig::at("/etc/tool.exe"), enter())
        .await
        .expect_err("a program");
    assert_eq!(refused.code(), "remote_edit_program");
    // F4 edits it: the editor runs, not the file.
    rig.engine
        .edit_remote(Rig::at("/etc/deploy"), editor("vi"))
        .await
        .expect("edited");

    rig.engine
        .set_protected_setting(REMOTE_EDIT_PROGRAMS_KEY, &json!(true))
        .await
        .expect("allowed");
    rig.engine
        .edit_remote(Rig::at("/etc/tool.exe"), enter())
        .await
        .expect("opened");
    assert_eq!(rig.list().len(), 2);
}

/// At most 32 copies; the 33rd is refused. Done removes a copy with its folder.
#[tokio::test]
async fn the_thirty_third_is_refused_and_done_removes_the_copy() {
    let files: Vec<(String, String)> = (0..=MAX_REMOTE_EDITS)
        .map(|n| (format!("/etc/f{n}.txt"), format!("{n}")))
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let rig = Rig::new(&borrowed).await;
    for (path, _) in files.iter().take(MAX_REMOTE_EDITS) {
        rig.engine
            .edit_remote(Rig::at(path), editor("vi"))
            .await
            .expect("opened");
    }
    let refused = rig
        .engine
        .edit_remote(Rig::at(&files[MAX_REMOTE_EDITS].0), editor("vi"))
        .await
        .expect_err("one too many");
    assert_eq!(refused.code(), "remote_edit_too_many");

    let first = rig.edit("f0.txt");
    let copy = rig.copy(first.id);
    rig.engine
        .dispatch(Command::CloseRemoteEdit { id: first.id })
        .await
        .expect("done");
    assert!(!copy.exists());
    assert!(!copy.parent().expect("folder").exists());
    assert_eq!(rig.list().len(), MAX_REMOTE_EDITS - 1);
}

/// A link on the server: the copy is of the file it leads to, which the upload replaces; the
/// list still names the link. A folder is no file to edit.
#[tokio::test]
async fn a_link_is_followed_to_its_file() {
    let rig = Rig::new(&[("/srv/site.conf", "listen 80")]).await;
    rig.server.link("/etc/site", "../srv/site.conf");
    let (remote, entry) = rig
        .engine
        .follow_links(&Rig::at("/etc/site"), "site")
        .await
        .expect("a file");
    assert_eq!(remote, Rig::at("/srv/site.conf"));
    assert_eq!(entry.size, 9);
    let folder = rig
        .engine
        .follow_links(&Rig::at("/srv"), "srv")
        .await
        .expect_err("a folder");
    assert_eq!(folder.code(), "remote_edit_not_a_file");
    rig.server.link("/etc/loop", "/etc/loop");
    let circle = rig
        .engine
        .follow_links(&Rig::at("/etc/loop"), "loop")
        .await
        .expect_err("a circle");
    assert_eq!(circle.code(), "remote_edit_not_a_file");
}

/// Over MCP a server file opens as a read-only copy nobody watches: an agent sets up no way
/// back to the server.
#[tokio::test]
async fn over_mcp_a_server_file_is_a_read_only_copy_without_an_edit() {
    let rig = Rig::new(&[("/etc/app.conf", "port=1")]).await;
    as_mcp(
        rig.engine
            .edit_remote(Rig::at("/etc/app.conf"), OpenRemote::default()),
    )
    .await
    .expect("opened");
    assert!(rig.list().is_empty(), "no edit");
    let opened = LAUNCHED
        .lock_or_recover("test.launched")
        .iter()
        .rev()
        .find(|(file, _)| file.ends_with("app.conf") && !file.starts_with(rig.edits.path()))
        .map(|(file, _)| file.clone())
        .expect("launched");
    let metadata = std::fs::metadata(&opened).expect("the copy");
    assert!(metadata.permissions().readonly());
    let _ = std::fs::remove_dir_all(opened.parent().expect("its folder"));
}

/// The error a refusal answers keeps its code through `EngineError`.
#[test]
fn the_refusals_have_their_codes() {
    let error: EngineError = crate::RemoteEditError::TooMany.into();
    assert_eq!(error.code(), "remote_edit_too_many");
}
