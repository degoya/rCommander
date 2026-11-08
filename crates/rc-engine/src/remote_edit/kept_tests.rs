use std::{fs, path::PathBuf};

use crate::Command;
use crate::remote_edit::{
    RemoteEditQuestion, RemoteEditState, UploadChoice, folder, open::OpenRemote, test_support::Rig,
};

/// A process id no process has.
const ENDED: u32 = 3_999_999_999;

fn editor() -> OpenRemote {
    OpenRemote {
        program: Some("vi".into()),
        ..OpenRemote::default()
    }
}

/// The edit folders and session files under the rig's root.
fn under_root(rig: &Rig) -> Vec<PathBuf> {
    fs::read_dir(rig.edits.path())
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default()
}

/// Quits with `/etc/app.conf` saved as `text` and not uploaded, and moves what was kept to a
/// folder of an ended process, as the next start finds it.
async fn kept_at_quit(text: &str) -> Rig {
    let rig = Rig::new(&[("/etc/app.conf", "port=1"), ("/etc/other.txt", "o")]).await;
    for path in ["/etc/app.conf", "/etc/other.txt"] {
        rig.engine
            .edit_remote(Rig::at(path), editor())
            .await
            .expect("opened");
    }
    let id = rig.edit("app.conf").id;
    rig.save(id, text).await;
    let names: Vec<String> = rig
        .engine
        .unsaved_remote_edits()
        .into_iter()
        .map(|edit| edit.name)
        .collect();
    assert_eq!(names, ["app.conf"], "quitting names the changed one");

    assert_eq!(
        rig.engine.finish_remote_edits(),
        1,
        "the changed one is kept"
    );
    assert_eq!(rig.engine.remote_edit_count(), 0);
    let kept = folder::folders(rig.edits.path(), |_| true);
    assert_eq!(
        kept.len(),
        1,
        "the unchanged one went: {:?}",
        under_root(&rig)
    );
    let dir = &kept[0];
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    let random = name.rsplit_once('-').expect("random").1;
    let ended = dir.with_file_name(format!("edit-{ENDED}-{random}"));
    fs::rename(dir, &ended).expect("moved");
    fs::rename(folder::session_path(dir), folder::session_path(&ended)).expect("moved");
    rig
}

#[tokio::test]
async fn kept_edits_are_offered_at_the_next_start() {
    let rig = kept_at_quit("port=2").await;
    assert_eq!(rig.engine.restore_remote_edits().await, 1);
    let edit = rig.edit("app.conf");
    assert_eq!(edit.question, Some(RemoteEditQuestion::Kept));
    assert_eq!(edit.state, RemoteEditState::Changed);
    let copy = rig.copy(edit.id);
    assert_eq!(fs::read_to_string(&copy).expect("copy"), "port=2");
    let dir = copy.parent().expect("folder");
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    assert_eq!(
        folder::owner(name),
        Some(std::process::id()),
        "now this run's"
    );

    rig.engine
        .dispatch(Command::UploadRemoteEdit {
            id: edit.id,
            choice: UploadChoice::Upload,
            always: false,
        })
        .await
        .expect("upload");
    rig.until_server("/etc/app.conf", "port=2").await;
    // Done: the copy goes; nothing is left under the root.
    rig.until("app.conf", |edit| {
        matches!(edit.state, RemoteEditState::Uploaded { .. })
    })
    .await;
    rig.engine
        .dispatch(Command::CloseRemoteEdit { id: edit.id })
        .await
        .expect("done");
    assert!(under_root(&rig).is_empty(), "{:?}", under_root(&rig));
}

#[tokio::test]
async fn a_kept_edit_still_detects_a_server_change() {
    let rig = kept_at_quit("port=2").await;
    rig.server.put("/etc/app.conf", "port=9 (theirs)").await;
    assert_eq!(rig.engine.restore_remote_edits().await, 1);
    let id = rig.edit("app.conf").id;
    rig.engine
        .dispatch(Command::UploadRemoteEdit {
            id,
            choice: UploadChoice::Upload,
            always: false,
        })
        .await
        .expect("upload");
    rig.until("app.conf", |edit| {
        edit.question == Some(RemoteEditQuestion::ServerChanged)
    })
    .await;
    assert_eq!(rig.server.text("/etc/app.conf").await, "port=9 (theirs)");
}

/// Discard drops a kept copy; Later keeps it for the next quit to keep again.
#[tokio::test]
async fn discard_drops_a_kept_copy_and_later_keeps_it() {
    let rig = kept_at_quit("port=3").await;
    rig.engine.restore_remote_edits().await;
    let id = rig.edit("app.conf").id;
    rig.engine
        .dispatch(Command::SkipRemoteEdit { id })
        .await
        .expect("later");
    assert_eq!(rig.edit("app.conf").question, None);
    assert_eq!(rig.engine.finish_remote_edits(), 1, "kept again");

    let rig = kept_at_quit("port=4").await;
    rig.engine.restore_remote_edits().await;
    let id = rig.edit("app.conf").id;
    rig.engine
        .dispatch(Command::CloseRemoteEdit { id })
        .await
        .expect("discard");
    assert!(under_root(&rig).is_empty());
    assert_eq!(rig.server.text("/etc/app.conf").await, "port=1");
}

/// Quitting without a change asks nothing and leaves nothing under the root.
#[tokio::test]
async fn a_quit_without_a_change_leaves_nothing() {
    let rig = Rig::new(&[("/etc/app.conf", "port=1")]).await;
    rig.engine
        .edit_remote(Rig::at("/etc/app.conf"), editor())
        .await
        .expect("opened");
    assert!(rig.engine.unsaved_remote_edits().is_empty());
    assert_eq!(rig.engine.finish_remote_edits(), 0);
    assert!(under_root(&rig).is_empty(), "{:?}", under_root(&rig));
}

/// A kept copy that turns out unchanged, or whose session does not read, goes without a word.
#[tokio::test]
async fn an_unchanged_or_unreadable_kept_folder_goes() {
    let rig = Rig::new(&[]).await;
    let root = rig.edits.path();
    for random in ["broken", "nosession"] {
        let dir = root.join(format!("edit-{ENDED}-{random}"));
        fs::create_dir(&dir).expect("folder");
        fs::write(dir.join("a.txt"), b"x").expect("copy");
    }
    fs::write(
        folder::session_path(&root.join(format!("edit-{ENDED}-broken"))),
        b"not json",
    )
    .expect("session");
    assert_eq!(rig.engine.restore_remote_edits().await, 0);
    assert!(under_root(&rig).is_empty(), "{:?}", under_root(&rig));
}
