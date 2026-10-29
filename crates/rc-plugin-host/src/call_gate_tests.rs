use std::{sync::Arc, time::Duration};

use super::CallGate;

const SHORT: Duration = Duration::from_millis(50);

#[tokio::test]
async fn calls_beyond_the_limit_wait_until_one_ends() {
    let gate = Arc::new(CallGate::default());
    let first = gate.enter(2).await;
    let second = gate.enter(2).await;
    assert_eq!(gate.running(), 2);
    let waiting = tokio::spawn({
        let gate = Arc::clone(&gate);
        async move { gate.enter(2).await }
    });
    tokio::time::sleep(SHORT).await;
    assert!(!waiting.is_finished(), "a third call waits");
    drop(first);
    let third = tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("let in")
        .expect("task");
    assert_eq!(gate.running(), 2);
    drop((second, third));
    assert_eq!(gate.running(), 0);
}

#[tokio::test]
async fn a_limit_of_one_runs_the_calls_one_after_the_other() {
    let gate = Arc::new(CallGate::default());
    let first = gate.enter(1).await;
    assert!(
        tokio::time::timeout(SHORT, gate.enter(1)).await.is_err(),
        "the second call waits"
    );
    drop(first);
    let _second = tokio::time::timeout(SHORT, gate.enter(1))
        .await
        .expect("let in once the first ended");
    assert_eq!(gate.running(), 1);
}

/// A wait the caller gave up leaves no count behind and lets the next call in.
#[tokio::test]
async fn a_dropped_wait_takes_no_place() {
    let gate = Arc::new(CallGate::default());
    let first = gate.enter(1).await;
    assert!(tokio::time::timeout(SHORT, gate.enter(1)).await.is_err());
    assert_eq!(gate.running(), 1);
    drop(first);
    assert_eq!(gate.running(), 0);
    let _next = tokio::time::timeout(SHORT, gate.enter(0))
        .await
        .expect("a limit of zero is one");
}
