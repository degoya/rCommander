//! The window's steps of pasting an image as a path (RC-040-08): what the status line says when
//! the setting is off, the clipboard holds no image, the system has no reader or placing fails.

use rc_engine::{EngineEvent, TermId};

use super::ImageMsg;
use crate::app::{Effect, Message, harness::Harness, terminal_pane::TermMessage};

fn step(message: ImageMsg) -> Message {
    Message::Terminal(TermMessage::Image(message))
}

#[tokio::test]
async fn the_announced_paste_reads_the_setting_first() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let announced = Message::Engine(EngineEvent::PasteImage {
        terminal: TermId(1),
    });
    assert!(matches!(
        harness.app.apply(announced)[..],
        [Effect::Task(_)]
    ));
}

#[tokio::test]
async fn each_failure_is_said_in_the_status_line() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let terminal = TermId(1);
    let said = |harness: &Harness| harness.app.info.clone().unwrap_or_default();

    harness.app.apply(step(ImageMsg::Allowed(terminal, false)));
    assert!(
        said(&harness).contains("switched off"),
        "{}",
        said(&harness)
    );

    harness.app.apply(step(ImageMsg::Read(terminal, None)));
    assert!(said(&harness).contains("no image"), "{}", said(&harness));

    harness
        .app
        .apply(step(ImageMsg::Placed(terminal, Err("terminal_not_found"))));
    assert!(
        said(&harness).contains("could not be pasted"),
        "{}",
        said(&harness)
    );

    let effects = harness.app.apply(step(ImageMsg::Allowed(terminal, true)));
    if cfg!(windows) {
        assert!(
            matches!(effects[..], [Effect::Task(_)]),
            "the clipboard is read"
        );
    } else {
        assert!(effects.is_empty());
        assert!(
            said(&harness).contains("needs Windows"),
            "{}",
            said(&harness)
        );
    }
}
