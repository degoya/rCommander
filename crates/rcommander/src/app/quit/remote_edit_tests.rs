//! The window's side of the server files edited through copies (RC-110-01): one question at a
//! time, the box going with Upload, the safe defaults of Escape and Enter, Done's question, the
//! list and the quit's question naming the copies.

use rc_engine::{
    BackendId, Command, EngineEvent, RemoteEditId, RemoteEditInfo, RemoteEditQuestion,
    RemoteEditState, UploadChoice, VfsPath,
};

use super::{RemoteEditMsg, named, wrap};
use crate::app::{Effect, Message, harness::Harness, quit::QuitMsg};

fn edit(
    id: u64,
    name: &str,
    state: RemoteEditState,
    question: Option<RemoteEditQuestion>,
) -> RemoteEditInfo {
    RemoteEditInfo {
        id: RemoteEditId(id),
        name: name.into(),
        server: "web".into(),
        path: VfsPath {
            backend: BackendId::Sftp { connection_id: 3 },
            path: format!("/etc/{name}").into(),
        },
        state,
        question,
    }
}

fn key(keys: &str) -> Message {
    Message::Key(keys.parse().expect("a chord"))
}

/// The commands `effects` hand to the engine.
fn commands(effects: Vec<Effect>) -> Vec<Command> {
    effects
        .into_iter()
        .flat_map(|effect| match effect {
            Effect::Dispatch(_, commands) => commands,
            _ => Vec::new(),
        })
        .collect()
}

async fn harness_with(edits: Vec<RemoteEditInfo>) -> (tempfile::TempDir, Harness) {
    let (root, mut harness) = Harness::temp_with_db().await;
    harness.settle().await;
    harness.app.quit.remote.edits = edits;
    (root, harness)
}

/// After a save: one question; Upload with the box sends the box along, once.
#[tokio::test]
async fn upload_takes_the_box_along() {
    let saved = edit(
        1,
        "app.conf",
        RemoteEditState::Changed,
        Some(RemoteEditQuestion::Upload),
    );
    let (_root, mut harness) = harness_with(vec![saved]).await;
    assert!(harness.app.quit.remote.asks());
    assert!(harness.app.dialog_open(), "a question is a dialog");
    harness.app.apply(wrap(RemoteEditMsg::Always(true)));
    let sent = commands(harness.app.apply(wrap(RemoteEditMsg::Upload(
        RemoteEditId(1),
        UploadChoice::Upload,
    ))));
    assert_eq!(
        sent,
        [Command::UploadRemoteEdit {
            id: RemoteEditId(1),
            choice: UploadChoice::Upload,
            always: true,
        }]
    );
    assert!(!harness.app.quit.remote.asks(), "answered at once");
    assert!(!harness.app.quit.remote.always, "the box is used once");
}

/// Escape is Not now; Enter is Upload after a save, the safe Not now about a server change.
#[tokio::test]
async fn escape_is_not_now_and_enter_the_default() {
    let saved = edit(
        1,
        "a.conf",
        RemoteEditState::Changed,
        Some(RemoteEditQuestion::Upload),
    );
    let (_root, mut harness) = harness_with(vec![saved.clone()]).await;
    let sent = commands(harness.app.apply(key("Escape")));
    assert_eq!(
        sent,
        [Command::SkipRemoteEdit {
            id: RemoteEditId(1)
        }]
    );

    harness.app.quit.remote.edits = vec![saved];
    let sent = commands(harness.app.apply(key("Enter")));
    assert!(matches!(
        sent.as_slice(),
        [Command::UploadRemoteEdit {
            choice: UploadChoice::Upload,
            always: false,
            ..
        }]
    ));

    let changed = edit(
        2,
        "b.conf",
        RemoteEditState::Conflict,
        Some(RemoteEditQuestion::ServerChanged),
    );
    harness.app.quit.remote.edits = vec![changed];
    let sent = commands(harness.app.apply(key("Enter")));
    assert_eq!(
        sent,
        [Command::SkipRemoteEdit {
            id: RemoteEditId(2)
        }]
    );
    // No key reaches the panels below while it asks.
    harness.app.quit.remote.edits = vec![edit(
        3,
        "c",
        RemoteEditState::Changed,
        Some(RemoteEditQuestion::InPlace),
    )];
    assert!(commands(harness.app.apply(key("F7"))).is_empty());
    assert!(!harness.app.file_ops.modal());
}

/// A kept change: Upload, Discard (the copy goes) or Later.
#[tokio::test]
async fn a_kept_change_offers_upload_discard_and_later() {
    let kept = edit(
        4,
        "k.conf",
        RemoteEditState::Changed,
        Some(RemoteEditQuestion::Kept),
    );
    let (_root, mut harness) = harness_with(vec![kept.clone()]).await;
    let sent = commands(
        harness
            .app
            .apply(wrap(RemoteEditMsg::Close(RemoteEditId(4)))),
    );
    assert_eq!(
        sent,
        [Command::CloseRemoteEdit {
            id: RemoteEditId(4)
        }]
    );
    harness.app.quit.remote.edits = vec![kept];
    let sent = commands(harness.app.apply(key("Escape")));
    assert_eq!(
        sent,
        [Command::SkipRemoteEdit {
            id: RemoteEditId(4)
        }],
        "Later"
    );
}

/// The list: opened by the engine's event, the arrows move its cursor, Done on a copy with a
/// change asks first, Done on an unchanged one closes it at once.
#[tokio::test]
async fn the_list_and_done() {
    let (_root, mut harness) = harness_with(Vec::new()).await;
    harness
        .send(Message::Engine(EngineEvent::RemoteEditWindow))
        .await;
    assert_eq!(harness.app.quit.remote.list, Some(0), "open, empty");
    harness.app.quit.remote.edits = vec![
        edit(1, "a", RemoteEditState::Unchanged, None),
        edit(2, "b", RemoteEditState::Changed, None),
    ];
    harness.app.apply(key("ArrowDown"));
    assert_eq!(harness.app.quit.remote.list, Some(1));
    let sent = commands(
        harness
            .app
            .apply(wrap(RemoteEditMsg::Done(RemoteEditId(2)))),
    );
    assert!(sent.is_empty(), "asked first");
    assert_eq!(harness.app.quit.remote.discard, Some(RemoteEditId(2)));
    let sent = commands(harness.app.apply(key("Escape")));
    assert!(sent.is_empty(), "Escape keeps it");
    harness
        .app
        .apply(wrap(RemoteEditMsg::Done(RemoteEditId(2))));
    let sent = commands(harness.app.apply(wrap(RemoteEditMsg::Discard(true))));
    assert_eq!(
        sent,
        [Command::CloseRemoteEdit {
            id: RemoteEditId(2)
        }]
    );
    let sent = commands(
        harness
            .app
            .apply(wrap(RemoteEditMsg::Done(RemoteEditId(1)))),
    );
    assert_eq!(
        sent,
        [Command::CloseRemoteEdit {
            id: RemoteEditId(1)
        }]
    );
    harness.app.apply(key("Escape"));
    assert_eq!(harness.app.quit.remote.list, None, "Escape closes the list");
}

/// Quitting with a changed copy asks, naming it with its server; Cancel quits nothing; without
/// a change nothing is asked.
#[tokio::test]
async fn quitting_names_the_changed_copies() {
    let (_root, mut harness) = harness_with(vec![
        edit(1, "a.conf", RemoteEditState::Unchanged, None),
        edit(2, "b.conf", RemoteEditState::Changed, None),
    ])
    .await;
    harness
        .send(Message::Engine(EngineEvent::Quit { unfinished: 0 }))
        .await;
    assert_eq!(harness.app.quit.asking, Some(0), "asked");
    let names = harness.app.quit.kept.clone().expect("named");
    assert!(names.contains("b.conf") && names.contains("web"), "{names}");
    assert!(!names.contains("a.conf"), "{names}");
    harness.send(Message::Quit(QuitMsg::Cancel)).await;
    assert!(!harness.app.quit.leaving, "Cancel quits nothing");

    harness.app.quit.remote.edits = vec![edit(1, "a.conf", RemoteEditState::Unchanged, None)];
    harness
        .send(Message::Engine(EngineEvent::Quit { unfinished: 0 }))
        .await;
    assert_eq!(harness.app.quit.asking, None, "nothing to ask");
    assert!(harness.app.quit.leaving);
}

#[test]
fn the_question_names_three_and_counts_the_rest() {
    let edits: Vec<RemoteEditInfo> = (1..=5)
        .map(|n| edit(n, &format!("f{n}"), RemoteEditState::Changed, None))
        .collect();
    let borrowed: Vec<&RemoteEditInfo> = edits.iter().collect();
    let text = named(&borrowed);
    for shown in ["f1", "f2", "f3"] {
        assert!(text.contains(shown), "{text}");
    }
    assert!(!text.contains("f4"), "{text}");
    assert!(text.contains('2'), "two more: {text}");
}
