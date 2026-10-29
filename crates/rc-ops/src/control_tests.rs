use std::time::Duration;

use super::{Cancelled, Control};

#[tokio::test]
async fn a_checkpoint_waits_while_paused() {
    let control = Control::default();
    assert_eq!(control.checkpoint().await, Ok(()));
    assert!(control.pause());
    assert!(!control.pause(), "pausing twice changes nothing");
    let waiting = tokio::spawn({
        let control = control.clone();
        async move { control.checkpoint().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!waiting.is_finished(), "paused: the checkpoint waits");
    assert!(control.resume());
    assert_eq!(waiting.await.expect("joins"), Ok(()));
}

#[tokio::test]
async fn cancel_ends_a_pause() {
    let control = Control::default();
    control.pause();
    let waiting = tokio::spawn({
        let control = control.clone();
        async move { control.checkpoint().await }
    });
    control.cancel();
    assert_eq!(waiting.await.expect("joins"), Err(Cancelled));
    control.cancelled().await;
}

#[tokio::test]
async fn a_held_job_waits_for_its_release() {
    let control = Control::default();
    assert_eq!(control.released().await, Ok(()));
    assert!(control.hold(true));
    assert!(!control.hold(true), "holding twice changes nothing");
    assert!(control.is_held());
    let waiting = tokio::spawn({
        let control = control.clone();
        async move { control.released().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!waiting.is_finished(), "held: it waits");
    assert_eq!(control.checkpoint().await, Ok(()), "a hold is no pause");
    assert!(control.hold(false));
    assert_eq!(waiting.await.expect("joins"), Ok(()));
    control.hold(true);
    control.cancel();
    assert_eq!(control.released().await, Err(Cancelled));
}

#[tokio::test]
async fn or_cancel_runs_the_work_until_a_cancel_comes_first() {
    let control = Control::default();
    assert_eq!(control.or_cancel(async { 7 }).await, Ok(7));
    let waiting = tokio::spawn({
        let control = control.clone();
        async move { control.or_cancel(std::future::pending::<()>()).await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    control.cancel();
    assert_eq!(waiting.await.expect("joins"), Err(Cancelled));
    assert_eq!(
        control.or_cancel(async { 7 }).await,
        Err(Cancelled),
        "the cancel is looked at first"
    );
}
