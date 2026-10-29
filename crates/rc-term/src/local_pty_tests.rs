//! A real shell in a real PTY, through the session (Unix; Windows is the manual acceptance).

use std::{path::PathBuf, time::Duration};

use crate::{
    LocalCommand, SessionOptions, SessionState, TermId, TermSession, TermSize, spawn_local,
};

/// `/bin/sh` in a PTY, started in a temporary folder.
fn sh(size: TermSize) -> (TermSession, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp");
    let command = LocalCommand {
        program: PathBuf::from("/bin/sh"),
        args: Vec::new(),
        cwd: Some(dir.path().to_path_buf()),
        // A fixed prompt, so no line of the screen is a surprise.
        env: vec![("PS1".to_owned(), "$ ".to_owned())],
        end_programs_on_close: true,
    };
    let spawned = spawn_local(&command, size).expect("spawn");
    let session = TermSession::start(
        TermId::next(),
        spawned,
        SessionOptions {
            title: "sh".into(),
            shell: "sh".into(),
            cwd: dir.path().to_path_buf(),
            size,
            scrollback: 1000,
        },
    );
    (session, dir)
}

/// Waits until a screen line is exactly `line`, for at most ten seconds.
async fn shows_line(session: &TermSession, line: &str) {
    for _ in 0..1000 {
        if session.screen(100).lines.iter().any(|shown| shown == line) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{line:?} never appeared; the screen: {:#?}",
        session.screen(100).lines
    );
}

/// Waits for an empty prompt. Typed before it, the input is echoed ahead of the prompt and the
/// output lands on the prompt's line (`$ roundtrip`).
async fn prompt(session: &TermSession) {
    shows_line(session, "$").await;
}

async fn exit_code(session: &TermSession) -> Option<u32> {
    tokio::time::timeout(Duration::from_secs(10), session.wait_exit())
        .await
        .expect("the shell ends in time")
}

#[tokio::test]
async fn a_typed_command_runs_and_its_output_appears() {
    let (session, dir) = sh(TermSize::default());
    prompt(&session).await;
    // Printed through printf so the output line differs from the echoed command line.
    session
        .write(b"printf 'round%s\\n' trip; pwd -P\r".to_vec())
        .expect("write");
    shows_line(&session, "roundtrip").await;
    let folder = dir.path().canonicalize().expect("canonical");
    shows_line(&session, &folder.display().to_string()).await;
    session.write(b"exit\r".to_vec()).expect("write");
    assert_eq!(exit_code(&session).await, Some(0));
}

#[tokio::test]
async fn resize_reaches_the_program() {
    let (session, _dir) = sh(TermSize::default());
    prompt(&session).await;
    session.write(b"stty size\r".to_vec()).expect("write");
    shows_line(&session, "24 80").await;
    prompt(&session).await;
    session
        .resize(TermSize::new(30, 100).expect("size"))
        .expect("resize");
    session.write(b"stty size\r".to_vec()).expect("write");
    shows_line(&session, "30 100").await;
    let screen = session.screen(0);
    assert_eq!((screen.rows, screen.cols), (30, 100));
}

#[tokio::test]
async fn exit_3_is_reported_and_nothing_keeps_running() {
    let (session, _dir) = sh(TermSize::default());
    let pid = session.process_id().expect("a local program has a pid");
    session.write(b"exit 3\r".to_vec()).expect("write");
    assert_eq!(exit_code(&session).await, Some(3));
    assert_eq!(session.state(), SessionState::Exited { code: Some(3) });
    for _ in 0..500 {
        if session.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(session.is_finished(), "the pump has stopped");
    #[cfg(target_os = "linux")]
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "process {pid} is reaped, not a zombie"
    );
    let _ = pid;
}

#[tokio::test]
async fn shutdown_ends_a_running_shell() {
    let (session, _dir) = sh(TermSize::default());
    session.write(b"sleep 600\r".to_vec()).expect("write");
    session.shutdown();
    exit_code(&session).await;
    assert!(session.state().is_exited());
    assert!(session.write(b"x".to_vec()).is_err());
}
