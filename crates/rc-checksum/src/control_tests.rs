use std::time::Duration;

use super::*;
use crate::test_support;

#[test]
fn a_control_counts_and_cancels_for_every_clone() {
    let control = Control::new();
    let clone = control.clone();
    clone.advance(5);
    control.advance(7);
    assert_eq!(control.bytes_done(), 12);
    assert!(control.check().is_ok());
    clone.cancel();
    assert!(control.is_cancelled());
    assert_eq!(control.check().expect_err("cancelled").code(), "cancelled");
}

#[test]
fn a_control_follows_a_queue_token() {
    let token = CancellationToken::new();
    let control = Control::with_token(token.child_token());
    token.cancel();
    assert!(control.is_cancelled());
}

#[tokio::test]
async fn a_paused_run_waits_until_resumed_or_cancelled() {
    let control = Control::new();
    control.pause();
    assert!(control.is_paused());
    let waiting = tokio::spawn({
        let control = control.clone();
        async move { control.checkpoint().await }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(!waiting.is_finished(), "a paused run went on");
    control.resume();
    assert!(waiting.await.expect("the task ran").is_ok());

    control.pause();
    let waiting = tokio::spawn({
        let control = control.clone();
        async move { control.checkpoint().await }
    });
    control.cancel();
    let stopped = waiting.await.expect("the task ran");
    assert_eq!(stopped.expect_err("cancelled").code(), "cancelled");
}

#[test]
fn a_control_adds_up_what_a_run_plans_and_finishes() {
    let control = Control::new();
    control.plan(2, 10);
    control.clone().plan(1, 5);
    control.file_done();
    assert_eq!(
        (
            control.files_total(),
            control.bytes_total(),
            control.files_done()
        ),
        (3, 15, 1)
    );
}

/// The progress window's File bar (RC-010-07): the file at work counts its own bytes, from 0
/// again with each file, while the run's bytes go on.
#[test]
fn the_file_at_work_counts_its_own_bytes_from_zero() {
    let control = Control::new();
    assert_eq!(control.current_file(), None);
    control.begin_file(Path::new("/d/a.bin"), 10);
    control.clone().advance(4);
    assert_eq!(
        (
            control.current_file(),
            control.file_bytes_done(),
            control.file_bytes_total()
        ),
        (Some(PathBuf::from("/d/a.bin")), 4, 10)
    );
    control.begin_file(Path::new("/d/b.bin"), 20);
    control.advance(3);
    assert_eq!(
        (
            control.current_file(),
            control.file_bytes_done(),
            control.file_bytes_total()
        ),
        (Some(PathBuf::from("/d/b.bin")), 3, 20)
    );
    assert_eq!(control.bytes_done(), 7);
}

#[tokio::test]
async fn kept_files_stay_and_discarded_ones_go() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "kept", b"1");
    test_support::write(root.path(), "gone", b"2");
    let mut kept = Pending::new(&vfs);
    kept.push(root.path().join("kept"));
    kept.keep();
    let mut gone = Pending::new(&vfs);
    gone.push(root.path().join("gone"));
    gone.push(root.path().join("never-created"));
    gone.discard().await;
    assert_eq!(test_support::names(root.path()), vec!["kept"]);
}

/// A run's future dropped half way (an MCP call cancelled, a window closed) still removes what
/// it created.
#[tokio::test]
async fn a_dropped_run_removes_its_files_behind_it() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "half", b"1");
    let mut pending = Pending::new(&vfs);
    pending.push(root.path().join("half"));
    drop(pending);
    for _ in 0..200 {
        if !root.path().join("half").exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the dropped run's file is still there");
}
