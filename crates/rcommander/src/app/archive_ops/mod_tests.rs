//! The archive dialogs through `apply`, against an engine over a temporary folder.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, ConflictChoice, JobId, JobState, PanelId, Question};
use tempfile::TempDir;

use super::{ArchiveDialog, ArchiveMsg, FormatChoice, PackStep, Typed};
use crate::app::{
    Effect, Message,
    file_ops::{FileOpsMsg, FollowUp},
    harness::Harness,
};

/// The left panel on `root` (cursor on `notes.txt`, the only entry), the right one on `out`.
async fn harness(root: &Path) -> Harness {
    fs::write(root.join("notes.txt"), b"notes").expect("write");
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open(root, &root.join("out")).await;
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Left),
            target: rc_engine::CursorTarget::Name("notes.txt".into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
    harness
}

async fn send(harness: &mut Harness, message: ArchiveMsg) {
    harness.send(Message::Archive(message)).await;
}

async fn last_job(harness: &Harness) -> rc_engine::JobSnapshot {
    let engine = harness.app.engine();
    let id = engine.ops().jobs().last().map(|job| job.id).expect("a job");
    tokio::time::timeout(Duration::from_secs(20), engine.ops().wait(id))
        .await
        .expect("in time")
        .expect("known")
}

fn pack(harness: &Harness) -> &super::PackDialog {
    match &harness.app.archive {
        Some(ArchiveDialog::Pack(pack)) => pack,
        other => panic!("the pack dialog, got {other:?}"),
    }
}

/// Acceptance: Alt+F5 asks first, names the archive after the entry in the other panel, and
/// the format changes the ending.
#[tokio::test]
async fn alt_f5_opens_the_dialog_and_packs() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.press("Alt+F5").await;
    let expected = root.path().join("out").join("notes.zip");
    assert_eq!(pack(&harness).name, expected.display().to_string());
    assert!(!pack(&harness).settings.delete_sources);

    send(
        &mut harness,
        ArchiveMsg::Format(FormatChoice(rc_engine::PackFormat::SevenZ)),
    )
    .await;
    assert!(pack(&harness).name.ends_with("notes.7z"));
    harness.press("Enter").await;
    assert_eq!(pack(&harness).step, PackStep::Checking);
    // The window asks off its thread; the test answers for it.
    send(&mut harness, ArchiveMsg::Checked(false)).await;
    assert!(harness.app.archive.is_none());
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(root.path().join("out").join("notes.7z").is_file());
    harness.settle().await;
    assert!(
        lists(&harness, PanelId::Right, "notes.7z"),
        "the other panel was read again"
    );
}

/// Alt+Shift+F5 has "delete sources" ticked and asks once more; Escape goes back.
#[tokio::test]
async fn deleting_the_sources_is_asked_once_more() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.press("Alt+Shift+F5").await;
    assert!(pack(&harness).settings.delete_sources);
    harness.press("Enter").await;
    send(&mut harness, ArchiveMsg::Checked(false)).await;
    assert_eq!(pack(&harness).step, PackStep::Confirm { exists: false });
    harness.press("Escape").await;
    assert_eq!(pack(&harness).step, PackStep::Editing);
    harness.press("Escape").await;
    assert!(harness.app.archive.is_none());
    assert!(root.path().join("notes.txt").is_file());
}

/// Acceptance: Alt+F9 unpacks into the other panel; the password prompt answers the job, and
/// its text never shows in a message's `Debug`.
#[tokio::test]
async fn alt_f9_unpacks_and_the_password_prompt_answers() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/encrypted.7z");
    fs::copy(&fixture, root.path().join("secret.7z")).expect("copy");
    let engine = harness.app.engine().clone();
    engine
        .dispatch(Command::Refresh {
            panel: Some(PanelId::Left),
        })
        .await
        .expect("refresh");
    engine
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Left),
            target: rc_engine::CursorTarget::Name("secret.7z".into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;

    harness.press("Alt+F9").await;
    let Some(ArchiveDialog::Extract(extract)) = &harness.app.archive else {
        panic!("the unpack dialog");
    };
    assert_eq!(extract.archives, ["secret.7z"]);
    harness.press("Enter").await;
    // The job reaches its question on its own time.
    for _ in 0..500 {
        harness.settle().await;
        if matches!(harness.app.archive, Some(ArchiveDialog::Password(_))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let Some(ArchiveDialog::Password(prompt)) = &harness.app.archive else {
        panic!("the password prompt, got {:?}", harness.app.archive);
    };
    assert_eq!(prompt.archive, "secret.7z");
    let typed = ArchiveMsg::Password(Typed::from("secret"));
    assert!(!format!("{typed:?}").contains("secret"), "{typed:?}");
    send(&mut harness, typed).await;
    harness.press("Enter").await;
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let unpacked = root.path().join("out").join("secret.txt");
    assert_eq!(fs::read(unpacked).expect("unpacked"), b"7z secret\n");
}

/// `name` of the fixtures in the left panel, under its cursor.
async fn put_archive(harness: &mut Harness, root: &Path, name: &str) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join(name);
    fs::copy(&fixture, root.join(name)).expect("copy");
    let engine = harness.app.engine().clone();
    engine
        .dispatch(Command::Refresh {
            panel: Some(PanelId::Left),
        })
        .await
        .expect("refresh");
    engine
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Left),
            target: rc_engine::CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
}

/// Whether the panel's listing, as the engine holds it, has `name`.
fn lists(harness: &Harness, panel: PanelId, name: &str) -> bool {
    let state = harness.app.engine().panel(panel);
    state.entries.iter().any(|entry| entry.name == name)
}

/// The question the window shows, once the job asked it.
async fn shown_question(harness: &mut Harness) -> (JobId, Question) {
    for _ in 0..500 {
        harness.settle().await;
        if let Some(job) = harness.app.file_ops.jobs.asking()
            && let Some(question) = job.snapshot.question.clone()
        {
            return (job.id(), question);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the window shows no question");
}

/// The owner's Windows test (2026-10-18): OK in the unpack dialog did nothing visible. The
/// dialog sent its command as a plain one, so the window never followed the job: the other
/// panel was not read again and a question waited for nobody. OK submits a job the window
/// follows, with the archive under the cursor and the other panel's folder as the target.
#[tokio::test]
async fn confirming_the_unpack_dialog_submits_a_job_the_window_follows() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    put_archive(&mut harness, root.path(), "tree.zip").await;
    let out = root.path().join("out");

    harness.press("Alt+F9").await;
    let effects = harness.app.apply(Message::Archive(ArchiveMsg::Confirm));
    let [Effect::Ops(submit)] = effects.as_slice() else {
        panic!("one job to follow, got {} effects", effects.len());
    };
    let Command::ArchiveExtract {
        panel,
        archives,
        target,
        ..
    } = &submit.command
    else {
        panic!("an unpack, got {:?}", submit.command);
    };
    assert_eq!(*panel, Some(PanelId::Left));
    assert_eq!(archives, &["tree.zip"]);
    assert_eq!(target.as_deref(), Some(out.display().to_string().as_str()));
    assert_eq!(submit.panel, PanelId::Left);
    assert_eq!(submit.follow, FollowUp::Refresh);
    assert!(!submit.background, "in the progress window");
    assert!(harness.app.archive.is_none());

    harness.press("Alt+F9").await;
    harness.press("Enter").await;
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(out.join("hello.txt").is_file());
    assert!(out.join("docs").join("readme.md").is_file());
    harness.settle().await;
    assert!(
        lists(&harness, PanelId::Right, "hello.txt"),
        "the other panel shows what was unpacked"
    );
    assert!(harness.app.file_ops.jobs.asking().is_none());
}

/// A file that is there already is asked about in the window, as for a copy.
#[tokio::test]
async fn an_unpack_conflict_is_asked_in_the_window() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    put_archive(&mut harness, root.path(), "tree.zip").await;
    let existing = root.path().join("out").join("hello.txt");
    fs::write(&existing, b"old").expect("write");

    harness.press("Alt+F9").await;
    harness.press("Enter").await;
    let (job, question) = shown_question(&mut harness).await;
    assert!(
        matches!(question, Question::Conflict { .. }),
        "{question:?}"
    );
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Overwrite,
            for_all: true,
        }))
        .await;
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_ne!(fs::read(&existing).expect("read"), b"old");
}

/// What cannot be unpacked is asked about and then named in the panel's status line - never
/// dropped without a word.
#[tokio::test]
async fn an_unpack_failure_reaches_the_status_line() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    // The cursor is on `notes.txt`, which is no archive.
    harness.press("Alt+F9").await;
    harness.press("Enter").await;
    let (job, question) = shown_question(&mut harness).await;
    let Question::Error(failure) = question else {
        panic!("a failing file, got {question:?}");
    };
    assert_eq!(failure.code, "not_an_archive");
    harness
        .send(Message::FileOps(FileOpsMsg::Error {
            job,
            choice: rc_engine::ErrorChoice::Skip,
            all: false,
        }))
        .await;
    let done = last_job(&harness).await;
    assert_eq!(done.failures.len(), 1, "{done:?}");
    harness.settle().await;
    assert!(
        harness.app.pane(PanelId::Left).error.is_some(),
        "the failure is in the status line"
    );
}

/// RC-030-03: "skip all" in the conflict dialog answers every conflict of the unpack - asked
/// once; the files that were there stay as they were, the rest is unpacked.
#[tokio::test]
async fn skip_all_in_the_conflict_dialog_keeps_every_existing_file() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    put_archive(&mut harness, root.path(), "tree.zip").await;
    let out = root.path().join("out");
    fs::create_dir(out.join("docs")).expect("mkdir");
    fs::write(out.join("hello.txt"), b"old").expect("write");
    fs::write(out.join("docs").join("readme.md"), b"old").expect("write");

    harness.press("Alt+F9").await;
    harness.press("Enter").await;
    let (job, question) = shown_question(&mut harness).await;
    assert!(
        matches!(question, Question::Conflict { .. }),
        "{question:?}"
    );
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Skip,
            for_all: true,
        }))
        .await;
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(done.skipped, 2, "{done:?}");
    assert_eq!(fs::read(out.join("hello.txt")).expect("read"), b"old");
    assert_eq!(
        fs::read(out.join("docs").join("readme.md")).expect("read"),
        b"old"
    );
    assert!(out.join("empty.txt").is_file(), "the rest is unpacked");
    harness.settle().await;
    assert!(harness.app.file_ops.jobs.asking().is_none());
}

/// RC-080-38: on `..` without a selection Alt+F5 and Alt+F9 open nothing - not for the first
/// entry the engine's cursor is on, as in Total Commander; a selection is packed as before.
#[tokio::test]
async fn on_the_parent_row_without_a_selection_nothing_is_packed_or_unpacked() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.send(Message::Click(PanelId::Left, 0)).await;
    assert!(harness.app.pane(PanelId::Left).on_parent());
    harness.press("Alt+F5").await;
    assert!(harness.app.archive.is_none(), "{:?}", harness.app.archive);
    harness.press("Alt+F9").await;
    assert!(harness.app.archive.is_none(), "{:?}", harness.app.archive);

    harness
        .app
        .engine()
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["notes.txt".into()],
        })
        .await
        .expect("selects");
    harness.settle().await;
    harness.press("Alt+F5").await;
    assert_eq!(pack(&harness).sources, ["notes.txt"]);
}
