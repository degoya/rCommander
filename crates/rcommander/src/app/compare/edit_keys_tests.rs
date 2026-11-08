//! The edit mode through `apply`, against real files (RC-100-94): F6 and Alt+M, Enter in a
//! line, Alt+Right and the copy buttons, Ctrl+Z, Alt+Backspace and Ctrl+Y, Ctrl+S, and the
//! questions before closing, comparing again and writing over a file changed on disk. The iced
//! tasks do not run in the harness; the tests run the same calls and hand the answers over, as
//! those tasks do.

use std::fs;

use iced::keyboard::Modifiers;
use rc_engine::{
    Command, PanelId, VfsPath,
    compare::{CompareSide, Direction},
};
use tempfile::TempDir;

use super::{Answer, EditMsg, compute, save};
use crate::app::{
    Effect, Message,
    compare::{Ask, CompareMsg, Shown, Then, load},
    file_ops::FileOpsMsg,
    harness::Harness,
};

const LEFT: &str = "one\ntwo\nthree\n";
const RIGHT: &str = "one\nTWO\nthree\n";

fn trees() -> TempDir {
    crate::app::harness::tree(&[("left/a.txt", LEFT), ("right/a.txt", RIGHT)])
}

fn read(root: &TempDir, side: &str) -> String {
    fs::read_to_string(root.path().join(side).join("a.txt")).expect("read")
}

async fn send(harness: &mut Harness, message: EditMsg) {
    harness
        .send(Message::Compare(CompareMsg::Edit(message)))
        .await;
}

/// Runs the window's comparison and hands its answer over, as the task does.
async fn loaded(harness: &mut Harness) {
    let window = harness.app.compare.diff.as_ref().expect("open");
    let generation = window.generation;
    let result = load(
        harness.app.engine().clone(),
        window.left.clone(),
        window.right.clone(),
        window.options,
        window.cancel.clone(),
    )
    .await;
    harness
        .send(Message::Compare(CompareMsg::DiffLoaded(generation, result)))
        .await;
}

/// The diff of the text as it is now, handed over as the task does after a change.
async fn computed(harness: &mut Harness) {
    let window = harness.app.compare.diff.as_ref().expect("open");
    let session = window.session.as_ref().expect("text");
    let (generation, at) = (window.generation, session.generation());
    let diff = compute(session.files(), window.options).await;
    send(harness, EditMsg::Computed(generation, at, diff)).await;
}

/// The saves running, carried out and answered as their tasks do.
async fn saved(harness: &mut Harness) {
    let window = harness.app.compare.diff.as_ref().expect("open");
    let (generation, calls) = (window.generation, window.saving.clone());
    assert!(!calls.is_empty(), "a save runs");
    for call in calls {
        let side = call.side;
        let result = save(harness.app.engine().clone(), call).await;
        send(harness, EditMsg::Saved(generation, side, result)).await;
    }
}

/// The diff window on both `a.txt`, compared.
async fn opened(root: &TempDir) -> Harness {
    let mut harness = Harness::open(&root.path().join("left"), &root.path().join("right")).await;
    for panel in PanelId::BOTH {
        harness
            .app
            .engine()
            .dispatch(Command::SetCursor {
                panel: Some(panel),
                target: rc_engine::CursorTarget::Name("a.txt".into()),
            })
            .await
            .expect("cursor");
    }
    harness.settle().await;
    harness.press("Ctrl+Shift+F3").await;
    loaded(&mut harness).await;
    harness
}

fn edit_row(harness: &Harness) -> Option<(usize, CompareSide)> {
    let window = harness.app.compare.diff.as_ref()?;
    window.edit.map(|cursor| (cursor.row, cursor.side))
}

#[tokio::test]
async fn f6_edits_the_line_under_the_cursor_and_ctrl_s_saves_it() {
    let root = trees();
    let mut harness = opened(&root).await;
    harness.press("F6").await;
    assert_eq!(edit_row(&harness), Some((1, CompareSide::Left)));
    harness.press("Enter").await;
    let window = harness.app.compare.diff.as_ref().expect("open");
    assert_eq!(
        window.field.as_ref().map(|field| field.text.as_str()),
        Some("two")
    );
    send(&mut harness, EditMsg::Field("deux".into())).await;
    harness.press("Enter").await;
    computed(&mut harness).await;
    assert!(harness.app.compare.diff.as_ref().is_some_and(|w| w.dirty()));
    harness.press("Ctrl+s").await;
    saved(&mut harness).await;
    assert_eq!(read(&root, "left"), "one\ndeux\nthree\n");
    assert_eq!(
        read(&root, "right"),
        RIGHT,
        "the right side was not changed"
    );
    let window = harness.app.compare.diff.as_ref().expect("open");
    assert!(!window.dirty());
    harness.press("F6").await;
    assert_eq!(edit_row(&harness), None, "F6 again leaves the edit mode");
}

#[tokio::test]
async fn alt_m_and_alt_right_copy_a_block_and_the_undo_keys_take_it_back() {
    let root = trees();
    let mut harness = opened(&root).await;
    harness.press("Alt+m").await;
    assert!(
        edit_row(&harness).is_some(),
        "Alt+M switches the edit mode on"
    );
    let text = |harness: &Harness, side| {
        let window = harness.app.compare.diff.as_ref().expect("open");
        let session = window.session.as_ref().expect("text");
        String::from_utf8(session.encoded(side).expect("encodes")).expect("UTF-8")
    };
    harness.press("Alt+ArrowRight").await;
    assert_eq!(text(&harness, CompareSide::Right), LEFT);
    computed(&mut harness).await;
    harness.press("Ctrl+z").await;
    assert_eq!(text(&harness, CompareSide::Right), RIGHT);
    computed(&mut harness).await;
    harness.press("Ctrl+y").await;
    assert_eq!(text(&harness, CompareSide::Right), LEFT);
    computed(&mut harness).await;
    harness.press("Alt+Backspace").await;
    assert_eq!(text(&harness, CompareSide::Right), RIGHT);
    computed(&mut harness).await;
    // The button "Copy <-" copies the other way.
    send(&mut harness, EditMsg::Copy(Direction::RightToLeft)).await;
    assert_eq!(text(&harness, CompareSide::Left), RIGHT);
    assert_eq!(
        read(&root, "left"),
        LEFT,
        "nothing is written before a save"
    );
}

#[tokio::test]
async fn closing_or_comparing_again_with_changes_asks_first() {
    let root = trees();
    let mut harness = opened(&root).await;
    harness.press("F6").await;
    harness.press("Alt+ArrowRight").await;
    computed(&mut harness).await;
    harness.press("Escape").await;
    let asked = |harness: &Harness| harness.app.compare.diff.as_ref().and_then(|w| w.ask);
    assert_eq!(asked(&harness), Some(Ask::Unsaved(Then::Close)));
    harness.press("Escape").await;
    assert_eq!(asked(&harness), None, "Escape cancels the question");
    assert!(harness.app.compare.diff.is_some());

    // "Compare" with changes: Discard reads both files again.
    send(&mut harness, EditMsg::Reload).await;
    assert_eq!(asked(&harness), Some(Ask::Unsaved(Then::Reload)));
    send(&mut harness, EditMsg::Answer(Answer::Discard)).await;
    let window = harness.app.compare.diff.as_ref().expect("open");
    assert!(matches!(window.shown, Shown::Loading));
    assert!(window.session.is_none());
    loaded(&mut harness).await;
    assert_eq!(read(&root, "right"), RIGHT);

    // Closing with changes: Save writes them, then the window closes.
    assert!(edit_row(&harness).is_some(), "the edit mode stays on");
    harness.press("Alt+ArrowRight").await;
    computed(&mut harness).await;
    send(&mut harness, EditMsg::Answer(Answer::Save)).await;
    assert!(harness.app.compare.diff.is_some(), "no question was open");
    harness.send(Message::Compare(CompareMsg::DiffClose)).await;
    harness.press("Enter").await;
    saved(&mut harness).await;
    assert!(harness.app.compare.diff.is_none(), "closed after the save");
    assert_eq!(read(&root, "right"), LEFT);
}

#[tokio::test]
async fn a_file_changed_on_disk_is_overwritten_only_when_asked_to() {
    let root = trees();
    let mut harness = opened(&root).await;
    harness.press("F6").await;
    harness.press("Alt+ArrowRight").await;
    computed(&mut harness).await;
    fs::write(
        root.path().join("right").join("a.txt"),
        "someone else's text\n",
    )
    .expect("write");
    harness.press("Ctrl+s").await;
    saved(&mut harness).await;
    let window = harness.app.compare.diff.as_ref().expect("open");
    assert_eq!(window.ask, Some(Ask::Changed(CompareSide::Right)));
    assert_eq!(read(&root, "right"), "someone else's text\n");
    send(&mut harness, EditMsg::Answer(Answer::Overwrite)).await;
    saved(&mut harness).await;
    assert_eq!(read(&root, "right"), LEFT);
}

/// The overwrite question's Compare and a sync row go to `compare.by_content`, which starts the
/// external tool when one is set; with Shift held they open the diff window, as in Total
/// Commander.
#[tokio::test]
async fn shift_keeps_the_diff_window_for_a_pair_asked_for() {
    let root = trees();
    let mut harness = Harness::open(&root.path().join("left"), &root.path().join("right")).await;
    let (left, right) = (
        VfsPath::local(root.path().join("left").join("a.txt")),
        VfsPath::local(root.path().join("right").join("a.txt")),
    );
    let effects = harness.app.compare_pair(left.clone(), right.clone());
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Dispatch(_, commands)]
                if matches!(commands.as_slice(), [Command::CompareByContent(_)])
        ),
        "compare.by_content decides"
    );
    assert!(harness.app.compare.diff.is_none());
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(Modifiers::SHIFT)))
        .await;
    let effects = harness.app.compare_pair(left.clone(), right);
    assert!(matches!(effects.as_slice(), [Effect::Task(_)]));
    let window = harness.app.compare.diff.as_ref().expect("the diff window");
    assert_eq!(window.left, left);
}
