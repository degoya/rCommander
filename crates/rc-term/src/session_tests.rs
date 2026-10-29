//! The session over a fake back end: the pump, the terminal's replies, title, size and end,
//! with no PTY involved (the real one is in `local_pty_tests.rs`).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::sync::mpsc;

use super::{SessionOptions, SessionState, TermEvent, TermId, TermSession};
use crate::test_support::{self, FakeBackend};
use crate::{
    BackendEvent, FollowOutcome, FollowStatus, Platform, PromptState, ShellPaths, ShellSyntax,
    TermError, TermSize,
};

fn session() -> (TermSession, Arc<FakeBackend>, mpsc::Sender<BackendEvent>) {
    let (spawned, fake, program) = test_support::spawned();
    let session = TermSession::start(
        TermId::next(),
        spawned,
        SessionOptions {
            title: "fake".into(),
            shell: "fake".into(),
            cwd: PathBuf::from("/start"),
            size: TermSize { rows: 5, cols: 30 },
            scrollback: 100,
        },
    );
    (session, fake, program)
}

async fn output(program: &mpsc::Sender<BackendEvent>, bytes: &[u8]) {
    program
        .send(BackendEvent::Output(bytes.to_vec()))
        .await
        .expect("send");
}

/// Waits until `check` holds, for at most five seconds.
async fn eventually(mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the condition never held");
}

#[tokio::test]
async fn output_reaches_the_screen_and_viewers_hear_of_it() {
    let (session, _fake, program) = session();
    let mut events = session.subscribe();
    output(&program, b"hello\r\nworld").await;
    eventually(|| session.screen(0).lines[1] == "world").await;
    assert_eq!(session.screen(0).lines[0], "hello");
    assert!(matches!(events.recv().await, Ok(TermEvent::Wakeup)));
}

#[tokio::test]
async fn the_terminals_replies_go_back_to_the_program() {
    let (session, fake, program) = session();
    // Device status report: where is the cursor? (row 1, column 4 after "abc")
    output(&program, b"abc\x1b[6n").await;
    eventually(|| fake.written.lock().expect("lock").as_slice() == b"\x1b[1;4R").await;
    drop(session);
}

#[tokio::test]
async fn the_program_sets_and_resets_the_title() {
    let (session, _fake, program) = session();
    assert_eq!(session.title(), "fake");
    output(&program, b"\x1b]0;build: 3 of 5\x07").await;
    eventually(|| session.title() == "build: 3 of 5").await;
}

#[tokio::test]
async fn input_and_size_reach_the_back_end_and_the_terminal() {
    let (session, fake, _program) = session();
    session.write(b"ls\r".to_vec()).expect("write");
    assert_eq!(fake.written.lock().expect("lock").as_slice(), b"ls\r");
    let size = TermSize { rows: 10, cols: 40 };
    session.resize(size).expect("resize");
    assert_eq!(fake.sizes.lock().expect("lock").as_slice(), [size]);
    assert_eq!(session.size(), size);
    let screen = session.screen(0);
    assert_eq!((screen.rows, screen.cols), (10, 40));
}

#[tokio::test]
async fn the_end_is_reported_and_the_pump_stops() {
    let (session, fake, program) = session();
    let mut events = session.subscribe();
    output(&program, b"bye").await;
    program
        .send(BackendEvent::Exited(Some(3)))
        .await
        .expect("send");
    assert_eq!(session.wait_exit().await, Some(3));
    assert_eq!(session.state(), SessionState::Exited { code: Some(3) });
    assert_eq!(
        session.screen(0).lines[0],
        "bye",
        "output before the end is kept"
    );
    eventually(|| session.is_finished()).await;
    let mut saw_exit = false;
    while let Ok(event) = events.try_recv() {
        saw_exit |= matches!(event, TermEvent::Exited(Some(3)));
    }
    assert!(saw_exit);
    assert!(matches!(
        session.write(b"x".to_vec()),
        Err(TermError::Exited)
    ));
    assert!(matches!(
        session.resize(TermSize::default()),
        Err(TermError::Exited)
    ));
    drop(session);
    assert!(*fake.shut_down.lock().expect("lock"), "dropping ends it");
}

#[tokio::test]
async fn a_back_end_that_vanishes_ends_the_session_without_a_code() {
    let (session, _fake, program) = session();
    drop(program);
    assert_eq!(session.wait_exit().await, None);
    assert_eq!(session.state(), SessionState::Exited { code: None });
}

#[test]
fn the_state_serializes_with_a_status() {
    let exited = serde_json::to_value(SessionState::Exited { code: Some(3) }).expect("json");
    assert_eq!(exited, serde_json::json!({ "status": "exited", "code": 3 }));
    let running = serde_json::to_value(SessionState::Running).expect("json");
    assert_eq!(running, serde_json::json!({ "status": "running" }));
}

#[tokio::test]
async fn the_folder_is_kept_and_can_follow_the_panel() {
    let (session, _fake, _program) = session();
    assert_eq!(session.cwd(), PathBuf::from("/start"));
    session.set_cwd(&PathBuf::from("/elsewhere"));
    assert_eq!(session.info().cwd, PathBuf::from("/elsewhere"));
}

/// RC-040-04 with a fake program: a `cd` is typed only at a marked prompt, never into a running
/// program, and a waiting one is typed when the prompt comes back.
#[tokio::test]
async fn the_shell_follows_the_panel_only_at_its_prompt() {
    let (session, fake, program) = session();
    session
        .sync()
        .configure(Some(ShellSyntax::Posix), ShellPaths::Native(Platform::Unix));
    let written = || fake.written.lock().expect("lock").clone();
    let mut events = session.subscribe();
    // No prompt mark yet: nothing is typed.
    assert_eq!(
        session.follow(Path::new("/elsewhere")).expect("follow"),
        FollowOutcome::Waiting(FollowStatus::NoIntegration)
    );
    assert!(written().is_empty());
    // The first prompt: the waiting cd goes out, once.
    output(&program, b"]7;file:///start]133;A$ ").await;
    eventually(|| written() == b" cd -- '/elsewhere'\r").await;
    assert_eq!(session.prompt_state(), PromptState::Running);
    output(&program, b"]7;file:///elsewhere\\]133;A\\$ ").await;
    eventually(|| session.reported_folder() == Some(PathBuf::from("/elsewhere"))).await;
    assert_eq!(session.cwd(), PathBuf::from("/elsewhere"));
    assert_eq!(written(), b" cd -- '/elsewhere'\r", "no second cd");
    // A program runs: navigating meanwhile types nothing into it.
    session.write(b"vim notes\r".to_vec()).expect("write");
    fake.written.lock().expect("lock").clear();
    assert_eq!(
        session.follow(Path::new("/third")).expect("follow"),
        FollowOutcome::Waiting(FollowStatus::Running)
    );
    output(&program, b"]133;C~\r\n~\r\n").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(written().is_empty(), "nothing typed into vim");
    // Back at the prompt: now it goes out.
    output(&program, b"]133;A$ ").await;
    eventually(|| written() == b" cd -- '/third'\r").await;
    let mut seen = (false, false);
    while let Ok(event) = events.try_recv() {
        seen.0 |= matches!(event, TermEvent::Folder);
        seen.1 |= matches!(event, TermEvent::Prompt);
    }
    assert_eq!(seen, (true, true));
}

#[test]
fn ids_are_fresh_and_sizes_are_never_empty() {
    assert_ne!(TermId::next(), TermId::next());
    assert!(matches!(TermSize::new(0, 80), Err(TermError::InvalidSize)));
    assert!(matches!(TermSize::new(24, 0), Err(TermError::InvalidSize)));
    // RC-080-42: no grid of 65535 by 65535 cells.
    assert!(matches!(
        TermSize::new(TermSize::MAX + 1, 80),
        Err(TermError::InvalidSize)
    ));
    assert!(matches!(
        TermSize::new(24, u16::MAX),
        Err(TermError::InvalidSize)
    ));
    assert!(TermSize::new(TermSize::MAX, TermSize::MAX).is_ok());
    assert_eq!(
        TermSize::new(24, 80).expect("size"),
        TermSize { rows: 24, cols: 80 }
    );
}
