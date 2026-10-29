use crate::test_support::engine;
use crate::{Command, Outcome, PanelId};

/// With nothing waiting, Escape's command changes nothing: the panel is where it was.
#[tokio::test]
async fn cancel_io_with_nothing_waiting_leaves_the_panel() {
    let (_root, engine) = engine(&[]).await;
    assert!(!engine.io_stalled());
    let answer = engine
        .dispatch(Command::CancelIo {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("cancel");
    let Outcome::Panel(snapshot) = answer else {
        panic!("a panel, got {answer:?}");
    };
    assert_eq!(snapshot.panel, PanelId::Right);
    assert_eq!(snapshot.path.backend, rc_vfs::BackendId::Local);
    assert!(!engine.io_stalled());
}

/// The panels' backend answers to the engine's switch: a listing after a cancel runs as usual.
#[tokio::test]
async fn the_panels_list_as_usual_after_a_cancel() {
    let (root, engine) = engine(&[]).await;
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    assert_eq!(engine.inner.io_cancel.cancel(), 0);
    let listed = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some(root.path().display().to_string()),
        })
        .await
        .expect("list");
    let Outcome::Listing { entries, .. } = listed else {
        panic!("a listing, got {listed:?}");
    };
    assert!(entries.iter().any(|entry| entry.name == "a.txt"));
}
