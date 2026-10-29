use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::mpsc;

use crate::test_support::{self, FakeBackend};
use crate::{BackendEvent, SessionOptions, TermError, TermId, TermSession, TermSize};

fn session() -> (TermSession, Arc<FakeBackend>, mpsc::Sender<BackendEvent>) {
    let (spawned, fake, program) = test_support::spawned();
    let session = TermSession::start(
        TermId::next(),
        spawned,
        SessionOptions {
            title: "fake".into(),
            shell: "fake".into(),
            cwd: PathBuf::from("/"),
            size: TermSize { rows: 5, cols: 40 },
            scrollback: 10,
        },
    );
    (session, fake, program)
}

#[tokio::test]
async fn held_input_goes_to_the_hook_and_the_terminals_replies_still_go_out() {
    let (session, fake, program) = session();
    let typed = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&typed);
    session.hold_input(move |bytes| seen.lock().expect("lock").extend_from_slice(bytes));
    assert!(session.input_held());
    assert!(matches!(
        session.write(b"\x03".to_vec()),
        Err(TermError::Busy)
    ));
    assert_eq!(*typed.lock().expect("lock"), b"\x03");
    // A cursor position request: the terminal answers it although input is held.
    program
        .send(BackendEvent::Output(b"\x1b[6n".to_vec()))
        .await
        .expect("sent");
    for _ in 0..500 {
        if !fake.written.lock().expect("lock").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(*fake.written.lock().expect("lock"), b"\x1b[1;1R");

    session.release_input();
    assert!(!session.input_held());
    session.write(b"ls\r".to_vec()).expect("typed");
    assert_eq!(*fake.written.lock().expect("lock"), b"\x1b[1;1Rls\r");
}

#[tokio::test]
async fn a_local_notice_shows_and_sends_nothing() {
    let (session, fake, _program) = session();
    session.show_local("\r\nlogin script timed out\r\n");
    assert_eq!(session.screen(0).lines[1], "login script timed out");
    assert!(fake.written.lock().expect("lock").is_empty());
}
