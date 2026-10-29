//! The password prompt against an engine whose jobs really ask (RC-080-44): a question waits
//! for the open archive dialog, and two questions are asked one after the other.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, CursorTarget, ExtractSettings, JobId, JobOptions, PanelId, Question};
use tempfile::TempDir;

use super::super::{ArchiveDialog, ArchiveMsg};
use crate::app::{Message, harness::Harness};

/// The left panel on `root` with an encrypted 7z archive under each of `names`, the cursor on
/// the first; the right one on `out`.
async fn harness(root: &Path, names: &[&str]) -> Harness {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/encrypted.7z");
    for name in names {
        fs::copy(&fixture, root.join(name)).expect("copy");
    }
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open(root, &root.join("out")).await;
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Left),
            target: CursorTarget::Name(names[0].into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
    harness
}

/// `archive` unpacked by a job the window does not follow, into its own folder of `out`.
async fn unpack(harness: &Harness, root: &Path, archive: &str) {
    let target = root.join("out").join(archive.replace('.', "-"));
    harness
        .app
        .engine()
        .dispatch(Command::ArchiveExtract {
            panel: Some(PanelId::Left),
            archives: vec![archive.to_owned()],
            target: Some(target.display().to_string()),
            settings: ExtractSettings::default(),
            options: JobOptions::default(),
        })
        .await
        .expect("an unpack job");
}

/// The jobs of the queue that ask for a password, once there are `count`; the app has seen
/// every question by then.
async fn asking(harness: &mut Harness, count: usize) -> Vec<JobId> {
    for _ in 0..500 {
        harness.settle().await;
        let jobs: Vec<JobId> = harness
            .app
            .engine()
            .ops()
            .jobs()
            .into_iter()
            .filter(|job| matches!(job.question, Some(Question::Password { .. })))
            .map(|job| job.id)
            .collect();
        if jobs.len() == count {
            harness.settle().await;
            return jobs;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no {count} jobs ask for a password");
}

fn prompted(harness: &Harness) -> Option<JobId> {
    match &harness.app.archive {
        Some(ArchiveDialog::Password(prompt)) => Some(prompt.job),
        _ => None,
    }
}

/// The audit's D8: a password question replaced the open pack dialog and what was typed in it.
/// Now it waits, and the prompt opens once the dialog closed.
#[tokio::test]
async fn a_password_question_waits_for_the_open_pack_dialog() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), &["secret.7z"]).await;
    harness.press("Alt+F5").await;
    harness
        .send(Message::Archive(ArchiveMsg::Name("kept.zip".into())))
        .await;

    unpack(&harness, root.path(), "secret.7z").await;
    let jobs = asking(&mut harness, 1).await;
    match &harness.app.archive {
        Some(ArchiveDialog::Pack(pack)) => assert_eq!(pack.name, "kept.zip"),
        other => panic!("the pack dialog as it was, got {other:?}"),
    }

    harness.send(Message::Archive(ArchiveMsg::Close)).await;
    assert_eq!(
        prompted(&harness),
        Some(jobs[0]),
        "{:?}",
        harness.app.archive
    );
}

/// The audit's D8: a second job's question replaced the first one's prompt, and the first job
/// waited for ever. Both are asked, one after the other; none is dropped.
#[tokio::test]
async fn two_password_questions_are_asked_one_after_the_other() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path(), &["one.7z", "two.7z"]).await;
    // Both jobs run at once, as a local unpack does beside one onto a server.
    harness
        .app
        .engine()
        .ops()
        .set_scheduling(rc_ops::Scheduling::Parallel);
    unpack(&harness, root.path(), "one.7z").await;
    unpack(&harness, root.path(), "two.7z").await;
    let jobs = asking(&mut harness, 2).await;

    let first = prompted(&harness).expect("a prompt for one of them");
    assert!(jobs.contains(&first), "{jobs:?}");
    // Escape skips that archive; the other job's question is asked next.
    harness.send(Message::Archive(ArchiveMsg::Close)).await;
    let second = prompted(&harness).expect("the other job's prompt");
    assert_ne!(first, second);
    assert!(jobs.contains(&second), "{jobs:?}");

    harness.send(Message::Archive(ArchiveMsg::Close)).await;
    assert_eq!(prompted(&harness), None, "nothing waits any more");
}
