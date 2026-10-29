//! The focus ring (RC-080-16) of the dialogs on the files under the cursor: copy, move and copy
//! here, the delete question, the conflict and error questions and the progress window,
//! multi-rename, properties and create link, pack, unpack and an archive's password. Each walks
//! round every stop in reading order, Enter and Space press each button and switch, and no button
//! is left out but the X of the head.

use std::time::Duration;

use iced::{Element, keyboard::key::Named, widget::Space};
use rc_engine::{ConflictChoice, ErrorChoice, Failure, Question, VfsPath};

use super::probe::{
    Screen, beside_a_file, first_of, kinds, press_at, press_every_stop, press_on, two_trees,
    walk_round,
};
use crate::{
    app::{
        Message,
        archive_ops::{self, ArchiveDialog, ArchiveMsg, PackOption, PasswordPrompt, Typed},
        file_ops::FileOpsMsg,
        harness::Harness,
        multi_rename::{self, Flag, MrMsg},
        properties::{self, PropsMsg},
    },
    theme::{self, focus::Kind},
};

/// The file operations' layer alone: a question, else the dialog, else the progress window.
fn file_ops_layer(harness: &Harness) -> Element<'_, Message> {
    let base: Element<'_, Message> = Space::new().into();
    harness.app.file_ops_layer(base, theme::tokens(false))
}

#[tokio::test]
async fn copy_move_and_copy_here_walk_their_target_options_and_buttons() {
    let (_root, mut harness) = beside_a_file().await;
    for key in ["F5", "F6", "Shift+F5"] {
        harness.press(key).await;
        {
            let mut screen = Screen::new(file_ops_layer(&harness)).await;
            let stops = walk_round(&mut screen, key);
            assert_eq!(
                kinds(&stops)[..4],
                [
                    None,
                    Some(Kind::Choice),
                    Some(Kind::Toggle),
                    Some(Kind::Toggle)
                ],
                "{key}: the target, the presets, verify, keep"
            );
            assert_eq!(screen.loose_buttons(), 1, "{key}: only the head's X");
            press_every_stop(&mut screen, &stops, key);
            let verify = press_on(&mut screen, "Verify each file", Named::Space);
            assert!(
                matches!(verify[..], [Message::FileOps(FileOpsMsg::Verify(_))]),
                "{key}"
            );
            let preset = press_at(&mut screen, 1, Named::ArrowDown);
            assert!(
                matches!(preset[..], [Message::FileOps(FileOpsMsg::Preset(_))]),
                "{key}"
            );
            let queue = press_on(&mut screen, "Queue (F2)", Named::Enter);
            assert!(
                matches!(
                    queue[..],
                    [Message::FileOps(FileOpsMsg::Start { background: true })]
                ),
                "{key}"
            );
        }
        harness.send(Message::FileOps(FileOpsMsg::Close)).await;
        assert!(harness.app.file_ops.dialog.is_none(), "{key} closes");
    }
}

#[tokio::test]
async fn the_delete_question_walks_ok_queue_and_cancel() {
    let (_root, mut harness) = beside_a_file().await;
    harness.press("F8").await;
    let mut screen = Screen::new(file_ops_layer(&harness)).await;
    let stops = walk_round(&mut screen, "delete");
    assert_eq!(kinds(&stops), [Some(Kind::Button); 3], "OK, Queue, Cancel");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let ok = press_on(&mut screen, "OK", Named::Space);
    assert!(matches!(
        ok[..],
        [Message::FileOps(FileOpsMsg::Start { background: false })]
    ));
    let queue = press_on(&mut screen, "Queue (F2)", Named::Enter);
    assert!(matches!(
        queue[..],
        [Message::FileOps(FileOpsMsg::Start { background: true })]
    ));
    let cancel = press_on(&mut screen, "Cancel", Named::Enter);
    assert!(matches!(cancel[..], [Message::FileOps(FileOpsMsg::Close)]));
}

/// F5 onto a file the other side has: the job asks, the question has its answers, the name to
/// rename to, Compare and Cancel in the ring. The same job then stands in for the two other cards
/// of a running job: with a failed file's question in place of the conflict, and with no question
/// - the progress window.
#[tokio::test]
async fn the_questions_and_the_progress_window_of_a_job_walk_their_buttons() {
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
        let stops = walk_round(&mut screen, "conflict");
        assert_eq!(stops[0].kind, Some(Kind::Button), "Overwrite first");
        assert!(kinds(&stops).contains(&None), "the name to rename to");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "conflict");
        let skip = press_on(&mut screen, "Skip", Named::Enter);
        assert!(matches!(
            skip[..],
            [Message::FileOps(FileOpsMsg::Conflict {
                choice: ConflictChoice::Skip,
                for_all: false,
                ..
            })]
        ));
        let compare = press_on(&mut screen, "Compare", Named::Space);
        assert!(matches!(
            compare[..],
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
        let stops = walk_round(&mut screen, "error");
        assert_eq!(
            kinds(&stops),
            [Some(Kind::Button); 4],
            "Retry, Skip, Skip all, Cancel"
        );
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        let retry = press_on(&mut screen, "Retry", Named::Enter);
        assert!(matches!(
            retry[..],
            [Message::FileOps(FileOpsMsg::Error {
                choice: ErrorChoice::Retry,
                all: false,
                ..
            })]
        ));
    }
    let tracked = harness.app.file_ops.jobs.get_mut(job).expect("followed");
    tracked.snapshot.question = None;
    {
        let mut screen = Screen::new(file_ops_layer(&harness)).await;
        let stops = walk_round(&mut screen, "progress");
        assert_eq!(
            kinds(&stops),
            [Some(Kind::Button); 3],
            "Pause, Background, Cancel"
        );
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        let background = press_on(&mut screen, "Background", Named::Enter);
        assert!(matches!(
            background[..],
            [Message::FileOps(FileOpsMsg::Background(_))]
        ));
    }
    // The engine's job still waits on its conflict.
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Cancel,
            for_all: false,
        }))
        .await;
}

#[tokio::test]
async fn multi_rename_walks_its_masks_switches_lists_and_buttons() {
    let (_root, mut harness) = beside_a_file().await;
    harness.press("Ctrl+M").await;
    let dialog = harness.app.multi_rename.as_ref().expect("the dialog");
    let mut screen = Screen::new(multi_rename::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "multi-rename");
    assert_eq!(stops[0].kind, None, "the rename mask first");
    assert!(kinds(&stops).contains(&Some(Kind::Choice)), "the lists");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "multi-rename");
    let case = press_on(&mut screen, "^ Case", Named::Space);
    assert!(matches!(
        case[..],
        [Message::MultiRename(MrMsg::Flag(Flag::CaseSensitive, _))]
    ));
    let close = press_on(&mut screen, "Close (Esc)", Named::Enter);
    assert!(matches!(close[..], [Message::MultiRename(MrMsg::Close)]));
}

#[tokio::test]
async fn properties_and_create_link_walk_their_controls() {
    let (_root, mut harness) = beside_a_file().await;
    let tokens = theme::tokens(false);
    harness.press("Alt+Enter").await;
    {
        let dialog = harness.app.properties.as_ref().expect("the properties");
        assert!(dialog.report.is_some(), "read");
        let mut screen = Screen::new(properties::overlay(dialog, tokens)).await;
        let stops = walk_round(&mut screen, "properties");
        assert_eq!(stops[0].kind, Some(Kind::Toggle), "read-only first");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "properties");
        let readonly = press_on(&mut screen, "Read-only", Named::Space);
        assert!(matches!(
            readonly[..],
            [Message::Properties(PropsMsg::Form(_))]
        ));
        let close = press_on(&mut screen, "Close", Named::Enter);
        assert!(matches!(close[..], [Message::Properties(PropsMsg::Close)]));
    }
    harness.send(Message::Properties(PropsMsg::Close)).await;
    harness.press("Ctrl+Shift+F5").await;
    let dialog = harness.app.properties.as_ref().expect("create link");
    assert!(dialog.link.is_some(), "the link's form");
    let mut screen = Screen::new(properties::overlay(dialog, tokens)).await;
    let stops = walk_round(&mut screen, "create link");
    assert_eq!(
        kinds(&stops)[..2],
        [Some(Kind::Choice), None],
        "the kinds, the place"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "create link");
    let create = press_on(&mut screen, "Create", Named::Enter);
    assert!(matches!(
        create[..],
        [Message::Properties(PropsMsg::Confirm)]
    ));
}

#[tokio::test]
async fn pack_and_unpack_walk_their_fields_lists_switches_and_buttons() {
    let (_root, mut harness) = beside_a_file().await;
    let tokens = theme::tokens(false);
    harness.press("Alt+F5").await;
    {
        let dialog = harness.app.archive.as_ref().expect("pack");
        let mut screen = Screen::new(archive_ops::overlay(dialog, tokens)).await;
        let stops = walk_round(&mut screen, "pack");
        assert_eq!(
            kinds(&stops)[..3],
            [None, Some(Kind::Choice), Some(Kind::Choice)],
            "the name, the format, the level"
        );
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "pack");
        let format = press_at(&mut screen, 1, Named::ArrowDown);
        assert!(matches!(
            format[..],
            [Message::Archive(ArchiveMsg::Format(_))]
        ));
        let recurse = press_on(&mut screen, "Include subfolders", Named::Space);
        assert!(matches!(
            recurse[..],
            [Message::Archive(ArchiveMsg::Option(PackOption::Recurse, _))]
        ));
    }
    harness.send(Message::Archive(ArchiveMsg::Close)).await;
    harness.press("Alt+F9").await;
    let dialog = harness.app.archive.as_ref().expect("unpack");
    let mut screen = Screen::new(archive_ops::overlay(dialog, tokens)).await;
    let stops = walk_round(&mut screen, "unpack");
    assert_eq!(kinds(&stops)[..2], [None, None], "the target, the mask");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "unpack");
    let conflict = press_at(
        &mut screen,
        first_of(&stops, Kind::Choice),
        Named::ArrowDown,
    );
    assert!(matches!(
        conflict[..],
        [Message::Archive(ArchiveMsg::Conflict(_))]
    ));
    let keep = press_on(&mut screen, "Keep folder paths", Named::Space);
    assert!(matches!(
        keep[..],
        [Message::Archive(ArchiveMsg::KeepPaths(_))]
    ));
    let cancel = press_on(&mut screen, "Cancel", Named::Enter);
    assert!(matches!(cancel[..], [Message::Archive(ArchiveMsg::Close)]));
    drop(screen);
    // An encrypted archive's password, asked by the job.
    harness.app.archive = Some(ArchiveDialog::Password(PasswordPrompt {
        job: 1,
        archive: "secret.zip".to_owned(),
        wrong: false,
        typed: Typed::default(),
    }));
    let dialog = harness.app.archive.as_ref().expect("the password");
    let mut screen = Screen::new(archive_ops::overlay(dialog, tokens)).await;
    let stops = walk_round(&mut screen, "archive password");
    assert_eq!(
        kinds(&stops),
        [None, Some(Kind::Button), Some(Kind::Button)],
        "the password, Unpack, Cancel"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let unpack = press_on(&mut screen, "Unpack", Named::Enter);
    assert!(matches!(
        unpack[..],
        [Message::Archive(ArchiveMsg::Confirm)]
    ));
}
