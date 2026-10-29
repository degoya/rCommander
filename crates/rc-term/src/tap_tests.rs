use super::tap;
use crate::BackendEvent;
use crate::test_support;

fn output(text: &str) -> BackendEvent {
    BackendEvent::Output(text.as_bytes().to_vec())
}

#[tokio::test]
async fn the_session_gets_every_event_and_the_tap_a_copy_of_the_output() {
    let (spawned, fake, program) = test_support::spawned();
    let (mut session, mut tapped) = tap(spawned);
    program.send(output("Password: ")).await.expect("sent");
    program
        .send(BackendEvent::Exited(Some(0)))
        .await
        .expect("sent");
    assert_eq!(session.events.recv().await, Some(output("Password: ")));
    assert_eq!(
        session.events.recv().await,
        Some(BackendEvent::Exited(Some(0)))
    );
    assert_eq!(tapped.read().await, Some(b"Password: ".to_vec()));
    assert_eq!(tapped.read().await, None, "the tap ends with the program");

    tapped.write(b"secret\r".to_vec()).expect("written");
    session.backend.write(b"ls\r".to_vec()).expect("written");
    assert_eq!(*fake.written.lock().expect("lock"), b"secret\rls\r");
}

#[tokio::test]
async fn a_dropped_tap_leaves_the_session_running() {
    let (spawned, _fake, program) = test_support::spawned();
    let (mut session, tapped) = tap(spawned);
    drop(tapped);
    program.send(output("one")).await.expect("sent");
    program.send(output("two")).await.expect("sent");
    assert_eq!(session.events.recv().await, Some(output("one")));
    assert_eq!(session.events.recv().await, Some(output("two")));
}

#[tokio::test]
async fn a_tap_that_falls_behind_misses_output_but_holds_up_nothing() {
    let (spawned, _fake, program) = test_support::spawned();
    let (mut session, mut tapped) = tap(spawned);
    let chunks = super::TAP_QUEUE + 10;
    let feed = tokio::spawn(async move {
        for index in 0..chunks {
            program
                .send(output(&index.to_string()))
                .await
                .expect("sent");
        }
    });
    for index in 0..chunks {
        assert_eq!(
            session.events.recv().await,
            Some(output(&index.to_string()))
        );
    }
    feed.await.expect("fed");
    let mut copied = 0;
    while let Ok(Some(_)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), tapped.read()).await
    {
        copied += 1;
    }
    assert_eq!(copied, super::TAP_QUEUE);
}
