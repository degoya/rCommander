use std::path::{Path, PathBuf};

use serde_json::json;

use crate::recover::LockOrRecover as _;
use crate::remote_edit::{
    RemoteEditId, RemoteEditQuestion, RemoteEditState, UploadChoice,
    open::OpenRemote,
    test_support::{Rig, Server},
};
use crate::{
    Command, CommandClass,
    settings::{REMOTE_EDIT_CHECK_KEY, REMOTE_EDIT_UPLOAD_KEY},
};

fn editor() -> OpenRemote {
    OpenRemote {
        program: Some("vi".into()),
        ..OpenRemote::default()
    }
}

/// A rig with `/etc/app.conf` open for editing; its id.
async fn opened(text: &str) -> (Rig, RemoteEditId) {
    let rig = Rig::new(&[("/etc/app.conf", text)]).await;
    rig.engine
        .edit_remote(Rig::at("/etc/app.conf"), editor())
        .await
        .expect("opened");
    let id = rig.edit("app.conf").id;
    (rig, id)
}

async fn upload(rig: &Rig, id: RemoteEditId, choice: UploadChoice) {
    rig.engine
        .dispatch(Command::UploadRemoteEdit {
            id,
            choice,
            always: false,
        })
        .await
        .expect("accepted");
}

fn uploaded(edit: &crate::RemoteEditInfo) -> bool {
    matches!(edit.state, RemoteEditState::Uploaded { .. })
}

fn renamed_onto(server: &Server, path: &str) -> usize {
    server
        .renames
        .lock_or_recover("test.renames")
        .iter()
        .filter(|(_, to)| to.as_path() == Path::new(path))
        .count()
}

/// `ask`: one question per save; Not now uploads nothing and the next save asks again; Upload
/// sends it up through a part file and a rename.
#[tokio::test]
async fn ask_puts_one_question_per_save() {
    let (rig, id) = opened("port=1").await;
    let mut events = rig.engine.subscribe();
    rig.save(id, "port=2").await;
    let edit = rig.edit("app.conf");
    assert_eq!(edit.question, Some(RemoteEditQuestion::Upload));
    assert_eq!(edit.state, RemoteEditState::Changed);
    let asked = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| {
            matches!(
                event,
                crate::EngineEvent::RemoteEdit {
                    change: crate::RemoteEditChange::Asked,
                    ..
                }
            )
        })
        .count();
    assert_eq!(asked, 1, "exactly one question");

    rig.engine
        .dispatch(Command::SkipRemoteEdit { id })
        .await
        .expect("not now");
    assert_eq!(rig.edit("app.conf").question, None);
    assert_eq!(
        rig.server.text("/etc/app.conf").await,
        "port=1",
        "nothing sent"
    );

    rig.save(id, "port=33").await;
    assert_eq!(
        rig.edit("app.conf").question,
        Some(RemoteEditQuestion::Upload)
    );
    upload(&rig, id, UploadChoice::Upload).await;
    rig.until("app.conf", uploaded).await;
    assert_eq!(rig.server.text("/etc/app.conf").await, "port=33");
    assert!(
        rig.server
            .writes
            .lock_or_recover("test.writes")
            .contains(&PathBuf::from("/etc/app.conf.rcpart")),
        "through a part file"
    );
    assert_eq!(
        renamed_onto(&rig.server, "/etc/app.conf"),
        1,
        "and a rename"
    );
}

/// The box "Upload changes without asking" sets `auto`; with it a save goes up at once.
#[tokio::test]
async fn the_box_switches_to_uploads_without_a_question() {
    let (rig, id) = opened("a").await;
    rig.save(id, "ab").await;
    assert_eq!(
        rig.edit("app.conf").question,
        Some(RemoteEditQuestion::Upload)
    );
    rig.engine
        .dispatch(Command::UploadRemoteEdit {
            id,
            choice: UploadChoice::Upload,
            always: true,
        })
        .await
        .expect("upload");
    rig.until_server("/etc/app.conf", "ab").await;
    assert!(rig.engine.uploads_without_asking().await, "auto now");

    rig.until("app.conf", uploaded).await;
    rig.save(id, "abc").await;
    assert_eq!(rig.edit("app.conf").question, None, "no question");
    rig.until_server("/etc/app.conf", "abc").await;
}

/// Two saves while an upload runs give exactly one upload after it.
#[tokio::test]
async fn saves_during_an_upload_give_one_upload_after_it() {
    let (rig, id) = opened("a").await;
    rig.engine
        .set_protected_setting(REMOTE_EDIT_UPLOAD_KEY, &json!("auto"))
        .await
        .expect("auto");
    // An upload runs: the saves wait for it.
    rig.engine
        .with_edit(id, |edit| edit.uploading = true)
        .expect("known");
    rig.save(id, "ab").await;
    rig.save(id, "abc").await;
    assert!(rig.engine.with_edit(id, |edit| edit.again).expect("known"));
    assert_eq!(
        renamed_onto(&rig.server, "/etc/app.conf"),
        0,
        "none while it runs"
    );
    // The running upload, then the one the saves asked for - and no third.
    rig.engine
        .clone()
        .upload_loop(id, UploadChoice::Upload)
        .await;
    assert_eq!(renamed_onto(&rig.server, "/etc/app.conf"), 2);
    assert_eq!(rig.server.text("/etc/app.conf").await, "abc");
    assert!(uploaded(&rig.edit("app.conf")));
}

/// A server file of mode 0600 is 0600 again after the upload (the rename would leave the
/// part's mode).
#[tokio::test]
async fn the_upload_gives_the_mode_back() {
    let rig = Rig::new(&[("/etc/secret.conf", "k=1")]).await;
    rig.server.set_mode("/etc/secret.conf", 0o600);
    rig.engine
        .edit_remote(Rig::at("/etc/secret.conf"), editor())
        .await
        .expect("opened");
    let id = rig.edit("secret.conf").id;
    rig.save(id, "k=22").await;
    upload(&rig, id, UploadChoice::Upload).await;
    rig.until("secret.conf", uploaded).await;
    assert_eq!(rig.server.text("/etc/secret.conf").await, "k=22");
    assert_eq!(rig.server.mode("/etc/secret.conf"), Some(0o600));
}

/// A server file changed since the fetch: the question, then Overwrite, Upload as copy or Not
/// now each do what they say.
#[tokio::test]
async fn a_server_change_asks_and_each_answer_does_its_part() {
    let (rig, id) = opened("mine").await;
    rig.server.put("/etc/app.conf", "theirs!").await;
    rig.save(id, "mine 2").await;
    upload(&rig, id, UploadChoice::Upload).await;
    rig.until("app.conf", |edit| {
        edit.question == Some(RemoteEditQuestion::ServerChanged)
    })
    .await;
    assert_eq!(rig.edit("app.conf").state, RemoteEditState::Conflict);
    assert_eq!(rig.server.text("/etc/app.conf").await, "theirs!");

    // Not now: nothing sent, the conflict stays.
    rig.engine
        .dispatch(Command::SkipRemoteEdit { id })
        .await
        .expect("not now");
    assert_eq!(rig.server.text("/etc/app.conf").await, "theirs!");

    // Upload as copy: beside it, as `name (2).ext`.
    upload(&rig, id, UploadChoice::AsCopy).await;
    rig.until("app.conf", uploaded).await;
    assert_eq!(rig.server.text("/etc/app (2).conf").await, "mine 2");
    assert_eq!(rig.server.text("/etc/app.conf").await, "theirs!");

    // Overwrite: over theirs.
    upload(&rig, id, UploadChoice::Overwrite).await;
    rig.until_server("/etc/app.conf", "mine 2").await;
    rig.until("app.conf", uploaded).await;
}

/// A server file gone since the fetch: Upload anyway puts it back.
#[tokio::test]
async fn a_gone_server_file_asks_upload_anyway() {
    let (rig, id) = opened("x").await;
    rig.server.remove("/etc/app.conf").await;
    rig.save(id, "xy").await;
    upload(&rig, id, UploadChoice::Upload).await;
    rig.until("app.conf", |edit| {
        edit.question == Some(RemoteEditQuestion::ServerGone)
    })
    .await;
    upload(&rig, id, UploadChoice::Overwrite).await;
    rig.until("app.conf", uploaded).await;
    assert_eq!(rig.server.text("/etc/app.conf").await, "xy");
}

/// With `remote_edit.check_server_changes` off nothing is looked at and nothing asked.
#[tokio::test]
async fn without_the_check_a_server_change_is_overwritten() {
    let (rig, id) = opened("mine").await;
    rig.engine
        .set_protected_setting(REMOTE_EDIT_CHECK_KEY, &json!(false))
        .await
        .expect("off");
    rig.server.put("/etc/app.conf", "theirs!").await;
    rig.save(id, "mine 2").await;
    upload(&rig, id, UploadChoice::Upload).await;
    rig.until("app.conf", uploaded).await;
    assert_eq!(rig.server.text("/etc/app.conf").await, "mine 2");
}

/// A link on the server: the upload replaces the file it leads to, the link stays.
#[tokio::test]
async fn a_symlink_keeps_pointing_after_the_upload() {
    let rig = Rig::new(&[("/srv/site.conf", "listen 80")]).await;
    rig.server.link("/etc/site", "../srv/site.conf");
    rig.engine
        .edit_remote(Rig::at("/etc/site"), editor())
        .await
        .expect("opened");
    let edit = rig.edit("site");
    assert_eq!(edit.path, Rig::at("/etc/site"), "the list names the link");
    rig.save(edit.id, "listen 8080").await;
    upload(&rig, edit.id, UploadChoice::Upload).await;
    rig.until("site", uploaded).await;
    assert_eq!(rig.server.text("/srv/site.conf").await, "listen 8080");
    assert_eq!(
        renamed_onto(&rig.server, "/etc/site"),
        0,
        "the link is not replaced"
    );
}

/// A file with another owner is written in place, after a question: no part file, no rename.
#[tokio::test]
async fn another_owner_asks_and_overwrites_in_place() {
    let (rig, id) = opened("a=1").await;
    rig.engine
        .with_edit(id, |edit| edit.in_place = true)
        .expect("known");
    rig.save(id, "a=2").await;
    upload(&rig, id, UploadChoice::Upload).await;
    rig.until("app.conf", |edit| {
        edit.question == Some(RemoteEditQuestion::InPlace)
    })
    .await;
    upload(&rig, id, UploadChoice::InPlace).await;
    rig.until("app.conf", uploaded).await;
    assert_eq!(rig.server.text("/etc/app.conf").await, "a=2");
    assert_eq!(
        renamed_onto(&rig.server, "/etc/app.conf"),
        0,
        "written in place"
    );
}

/// Upload, Not now and Done are the person's: MCP's dispatch refuses them.
#[tokio::test]
async fn the_decisions_are_the_windows_only() {
    let (rig, id) = opened("a").await;
    for command in [
        Command::UploadRemoteEdit {
            id,
            choice: UploadChoice::Upload,
            always: false,
        },
        Command::SkipRemoteEdit { id },
        Command::CloseRemoteEdit { id },
        Command::RemoteEditWindow {},
    ] {
        assert_eq!(command.spec().class, CommandClass::GuiOnly);
        let refused = rig
            .engine
            .dispatch_mcp(command)
            .await
            .expect_err("gui only");
        assert_eq!(refused.code(), "gui_only");
    }
    let listed = rig
        .engine
        .dispatch_mcp(Command::ListRemoteEdits {})
        .await
        .expect("read");
    let crate::Outcome::RemoteEdits { edits } = listed else {
        panic!("the list, got {listed:?}");
    };
    assert_eq!(edits.len(), 1);
}
