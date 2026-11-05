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

/// `/bin/sh -c script` in a PTY, started in `dir`; the back end and its events.
fn sh_c(script: &str, dir: &std::path::Path) -> crate::Spawned {
    let command = LocalCommand {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".to_owned(), script.to_owned()],
        cwd: Some(dir.to_path_buf()),
        env: Vec::new(),
        end_programs_on_close: true,
    };
    spawn_local(&command, TermSize::default()).expect("spawn")
}

/// The process id the script wrote into `name`, once it is there.
async fn pid_in(dir: &std::path::Path, name: &str) -> i32 {
    for _ in 0..1000 {
        if let Ok(text) = std::fs::read_to_string(dir.join(name))
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the script never wrote {name}");
}

/// Reads the events until the channel closes - the reader and the waiter thread have both
/// ended then, they hold its only senders -, for at most `limit`; whether `Exited` came.
async fn until_closed(
    mut events: tokio::sync::mpsc::Receiver<crate::BackendEvent>,
    limit: Duration,
) -> bool {
    let mut exited = false;
    tokio::time::timeout(limit, async {
        while let Some(event) = events.recv().await {
            exited |= matches!(event, crate::BackendEvent::Exited(_));
        }
    })
    .await
    .expect("the reader and the waiter end in time");
    exited
}

/// Whether process `pid` has ended (a zombie waiting for its reaper has).
fn gone(pid: i32) -> bool {
    #[cfg(target_os = "linux")]
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        return stat
            .rsplit(')')
            .next()
            .is_some_and(|rest| rest.trim_start().starts_with('Z'));
    }
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err()
}

/// Waits up to five seconds for process `pid` to end; whether it did.
async fn ends(pid: i32) -> bool {
    for _ in 0..500 {
        if gone(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn a_program_that_ignores_hangup_and_term_is_killed_with_its_group() {
    let dir = tempfile::tempdir().expect("temp");
    let spawned = sh_c(
        "trap '' HUP TERM; sleep 600 & echo $! > bg; sleep 600",
        dir.path(),
    );
    let background = pid_in(dir.path(), "bg").await;
    let started = std::time::Instant::now();
    spawned.backend.shutdown();
    assert!(until_closed(spawned.events, Duration::from_secs(20)).await);
    assert!(
        started.elapsed() >= super::ending::END_GRACE * 2,
        "only SIGKILL ends it, after both graces"
    );
    assert!(ends(background).await, "the background job ended with it");
}

#[tokio::test]
async fn a_program_that_ends_on_hangup_ends_at_once() {
    let dir = tempfile::tempdir().expect("temp");
    let spawned = sh_c("sleep 600 & echo $! > bg; sleep 600", dir.path());
    let background = pid_in(dir.path(), "bg").await;
    let started = std::time::Instant::now();
    spawned.backend.shutdown();
    assert!(until_closed(spawned.events, Duration::from_secs(10)).await);
    assert!(
        started.elapsed() < super::ending::END_GRACE,
        "no SIGTERM, so no SIGKILL either: {:?}",
        started.elapsed()
    );
    assert!(ends(background).await);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn sigterm_ends_the_group_but_not_a_detached_process() {
    let dir = tempfile::tempdir().expect("temp");
    let spawned = sh_c(
        "trap '' HUP; setsid sleep 600 </dev/null >/dev/null 2>&1 & echo $! > detached; \
         sleep 600 & echo $! > bg; sleep 600",
        dir.path(),
    );
    let detached = pid_in(dir.path(), "detached").await;
    let background = pid_in(dir.path(), "bg").await;
    spawned.backend.shutdown();
    assert!(until_closed(spawned.events, Duration::from_secs(20)).await);
    assert!(ends(background).await, "the background job ended with it");
    let survived = !gone(detached);
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(detached),
        nix::sys::signal::Signal::SIGKILL,
    );
    assert!(survived, "a process detached with setsid runs on");
}

#[test]
fn a_full_input_queue_refuses_a_write_rather_than_grow() {
    let (input, queued) = std::sync::mpsc::sync_channel(crate::INPUT_QUEUE);
    for _ in 0..crate::INPUT_QUEUE {
        super::queue(&input, b"x".to_vec()).expect("room");
    }
    assert!(matches!(
        super::queue(&input, b"x".to_vec()),
        Err(crate::TermError::InputFull)
    ));
    queued.recv().expect("the writer takes one");
    super::queue(&input, b"x".to_vec()).expect("room again");
    drop(queued);
    assert!(matches!(
        super::queue(&input, b"y".to_vec()),
        Err(crate::TermError::Exited)
    ));
}

#[test]
fn a_program_that_is_not_there_fails_with_its_path() {
    let command = LocalCommand {
        program: PathBuf::from("/nowhere/System32/wsl.exe"),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
        end_programs_on_close: true,
    };
    match spawn_local(&command, TermSize::default()) {
        Err(crate::TermError::Pty(message)) => {
            assert!(message.contains("/nowhere/System32/wsl.exe"), "{message}");
        }
        other => panic!("a start error, got {other:?}"),
    }
}
