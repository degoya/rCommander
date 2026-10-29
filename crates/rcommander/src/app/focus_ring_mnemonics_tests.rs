//! The access keys of the dialogs (RC-080-16), pressed as the window delivers them: Alt with the
//! letter presses its button from anywhere in the dialog and types nothing into a field; the
//! letter alone only while a button or a checkbox has the ring; a question over a dialog takes
//! its letters first; two buttons with one letter take turns with the ring. That no dialog has
//! a letter twice, `walk_round` checks for every dialog the ring tests open.

use std::time::Duration;

use iced::{Element, widget::column};
use rc_engine::{ConflictChoice, ErrorChoice, Failure, Question, VfsPath};

use super::{
    probe::{Screen, beside_a_file, two_trees, walk_round},
    ring_settings_tests::{settings_open, settings_window},
};
use crate::{
    app::{Message, file_ops::FileOpsMsg, harness::Harness, settings::SettingsMessage},
    theme::{
        self,
        layout::{self, Head},
    },
};

fn file_ops_layer(harness: &Harness) -> Element<'_, Message> {
    let base: Element<'_, Message> = iced::widget::Space::new().into();
    harness.app.file_ops_layer(base, theme::tokens(false))
}

/// A small card titled `title` round `content`.
fn card<'a>(title: &'a str, content: Element<'a, Message>) -> Element<'a, Message> {
    let tokens = theme::tokens(false);
    let head = Head::new(iced::widget::text(title), Message::Press);
    layout::dialog_card(head, content, layout::SMALL_DIALOG_WIDTH, tokens)
}

fn is_start(message: &Message, queued: bool) -> bool {
    matches!(message, Message::FileOps(FileOpsMsg::Start { background }) if *background == queued)
}

#[tokio::test]
async fn alt_with_the_letter_presses_queue_in_the_copy_dialog_and_types_nothing() {
    let (_root, mut harness) = beside_a_file().await;
    harness.press("F5").await;
    let mut screen = Screen::new(file_ops_layer(&harness)).await;
    let stops = walk_round(&mut screen, "copy");
    let queue = stops
        .iter()
        .position(|seen| seen.text.contains("Queue (F2)"))
        .expect("Queue");
    assert_eq!(stops[queue].letter, Some('q'));
    // The target field has the keyboard: the letter alone is its, Alt with it the button's.
    screen.tab(false);
    assert_eq!(screen.at(), Some(0), "the target");
    let (typed, _) = screen.type_letter('q', false);
    assert!(
        !typed.iter().any(|message| is_start(message, true)),
        "{typed:?}"
    );
    let (pressed, taken) = screen.type_letter('q', true);
    assert!(taken);
    assert!(
        matches!(pressed[..], [ref only] if is_start(only, true)),
        "{pressed:?}"
    );
    assert_eq!(screen.at(), Some(queue), "the ring moved to Queue");
    // On the pick list of presets the letter alone is the list's, not the button's.
    while screen.at() != Some(1) {
        screen.tab(false);
    }
    let (messages, taken) = screen.type_letter('q', false);
    assert!(messages.is_empty() && !taken, "{messages:?}");
    // A letter no button has goes on as it came.
    let (messages, taken) = screen.type_letter('z', true);
    assert!(messages.is_empty() && !taken, "{messages:?}");
}

#[tokio::test]
async fn the_letter_alone_presses_only_while_a_button_has_the_ring() {
    let (_root, mut harness) = beside_a_file().await;
    harness.press("F8").await;
    let mut screen = Screen::new(file_ops_layer(&harness)).await;
    walk_round(&mut screen, "delete");
    // Nothing has the ring yet: the question takes no letter by surprise.
    let mut fresh = Screen::new(file_ops_layer(&harness)).await;
    let (messages, taken) = fresh.type_letter('q', false);
    assert!(messages.is_empty() && !taken, "{messages:?}");
    // OK has it: Q is Queue's.
    fresh.tab(false);
    let (messages, taken) = fresh.type_letter('q', false);
    assert!(taken);
    assert!(
        matches!(messages[..], [ref only] if is_start(only, true)),
        "{messages:?}"
    );
}

/// F5 onto a file the other side has: the conflict question, then the same job's error question
/// and its progress window, each answering to its letters.
#[tokio::test]
async fn the_questions_and_the_progress_window_answer_to_their_letters() {
    let (root, mut harness) = two_trees().await;
    harness.press("F5").await;
    harness
        .send(Message::FileOps(FileOpsMsg::Start { background: false }))
        .await;
    let engine = harness.app.engine().clone();
    let job = engine.ops().jobs().last().map(|job| job.id).expect("a job");
    for _ in 0..500 {
        if engine.ops().job(job).expect("known").question.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    harness.settle().await;
    assert!(harness.app.file_ops.jobs.asking().is_some(), "the conflict");
    {
        let mut screen = Screen::new(file_ops_layer(&harness)).await;
        walk_round(&mut screen, "conflict");
        for (letter, wanted, all) in [
            ('o', ConflictChoice::Overwrite, false),
            ('a', ConflictChoice::Overwrite, true),
            ('s', ConflictChoice::Skip, false),
            ('k', ConflictChoice::Skip, true),
            ('d', ConflictChoice::OverwriteOlder, true),
            ('l', ConflictChoice::OverwriteLarger, true),
            ('m', ConflictChoice::OverwriteSmaller, true),
            ('r', ConflictChoice::Rename, false),
            ('n', ConflictChoice::Rename, true),
            ('e', ConflictChoice::RenameExisting, false),
        ] {
            let (messages, taken) = screen.type_letter(letter, true);
            assert!(taken, "Alt+{letter}");
            assert!(
                matches!(
                    messages[..],
                    [Message::FileOps(FileOpsMsg::Conflict { choice, for_all, .. })]
                        if choice == wanted && for_all == all
                ),
                "Alt+{letter}: {messages:?}"
            );
        }
        let (messages, _) = screen.type_letter('t', true);
        assert!(matches!(
            messages[..],
            [Message::FileOps(FileOpsMsg::RenameTo(_))]
        ));
        let (messages, _) = screen.type_letter('c', true);
        assert!(matches!(
            messages[..],
            [Message::FileOps(FileOpsMsg::Compare(_))]
        ));
    }
    let failure = Failure {
        path: VfsPath::local(root.path().join("left").join("diff.txt")),
        code: "not_found".to_owned(),
        message: String::new(),
        target: None,
    };
    let tracked = harness.app.file_ops.jobs.get_mut(job).expect("followed");
    tracked.snapshot.question = Some(Question::Error(failure));
    {
        let mut screen = Screen::new(file_ops_layer(&harness)).await;
        walk_round(&mut screen, "error");
        for (letter, wanted, every) in [
            ('r', ErrorChoice::Retry, false),
            ('s', ErrorChoice::Skip, false),
            ('k', ErrorChoice::Skip, true),
        ] {
            let (messages, _) = screen.type_letter(letter, true);
            assert!(
                matches!(
                    messages[..],
                    [Message::FileOps(FileOpsMsg::Error { choice, all, .. })]
                        if choice == wanted && all == every
                ),
                "Alt+{letter}: {messages:?}"
            );
        }
    }
    let tracked = harness.app.file_ops.jobs.get_mut(job).expect("followed");
    tracked.snapshot.question = None;
    {
        let mut screen = Screen::new(file_ops_layer(&harness)).await;
        walk_round(&mut screen, "progress");
        let (messages, _) = screen.type_letter('p', true);
        assert!(matches!(
            messages[..],
            [Message::FileOps(FileOpsMsg::Pause(_))]
        ));
        let (messages, _) = screen.type_letter('b', true);
        assert!(matches!(
            messages[..],
            [Message::FileOps(FileOpsMsg::Background(_))]
        ));
    }
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Cancel,
            for_all: false,
        }))
        .await;
}

#[tokio::test]
async fn alt_a_applies_the_settings() {
    let (_root, harness) = settings_open().await;
    let mut screen = Screen::new(settings_window(&harness)).await;
    let (messages, taken) = screen.type_letter('a', true);
    assert!(taken);
    assert!(
        matches!(messages[..], [Message::Settings(SettingsMessage::Apply)]),
        "{messages:?}"
    );
}

#[tokio::test]
async fn a_question_inside_a_dialog_takes_its_letters_first() {
    let inner = card(
        "Question",
        layout::secondary_button("&Stop", Some(Message::Press)),
    );
    let outer = card(
        "Dialog",
        column![
            layout::secondary_button("&Skip", Some(Message::Saved)),
            inner
        ]
        .into(),
    );
    let mut screen = Screen::new(outer).await;
    let (messages, taken) = screen.type_letter('s', true);
    assert!(taken);
    assert!(matches!(messages[..], [Message::Press]), "{messages:?}");
}

#[tokio::test]
async fn two_buttons_with_one_letter_take_turns_with_the_ring() {
    let both = card(
        "Twice",
        column![
            layout::secondary_button("&Save", Some(Message::Saved)),
            layout::secondary_button("&Skip", Some(Message::Press)),
        ]
        .into(),
    );
    let mut screen = Screen::new(both).await;
    for at in [0, 1, 0] {
        let (messages, taken) = screen.type_letter('s', true);
        assert!(taken && messages.is_empty(), "{messages:?}");
        assert_eq!(screen.at(), Some(at));
    }
}
