//! The SSH terminal as a person uses it (RC-060-03): full-screen programs on a real server - vim
//! and htop - draw, take keys and give the shell back, and a new size reaches the server. Every
//! key goes in by `terminal.send_input` and every screen comes back by `terminal.read_screen`,
//! the commands behind the MCP tools `terminal_send_input` and `terminal_read_screen`.

use std::time::{Duration, Instant};

use rc_engine::{Command, ConnectionId, Engine, Outcome, TermId, TermSize};

use crate::live::{connect_trusting, connection, engine, workdir};
use crate::live_or_skip;

/// An SSH terminal of `rows` by `cols` on the connection `id`.
pub async fn open(engine: &Engine, id: ConnectionId, rows: u16, cols: u16) -> TermId {
    let opened = engine
        .dispatch(Command::OpenSshTerminal {
            connection_id: id,
            panel: None,
            cwd: None,
            size: Some(TermSize { rows, cols }),
        })
        .await;
    let Ok(Outcome::Terminal(info)) = opened else {
        panic!("a terminal, got {opened:?}");
    };
    info.terminal
}

/// The screen's rows, by `terminal.read_screen`.
pub async fn screen(engine: &Engine, terminal: TermId) -> Vec<String> {
    let read = engine
        .dispatch(Command::ReadScreen {
            terminal,
            scrollback: 0,
        })
        .await;
    let Ok(Outcome::Screen { screen, .. }) = read else {
        panic!("a screen, got {read:?}");
    };
    screen.lines
}

/// Types `text` by `terminal.send_input`.
pub async fn type_in(engine: &Engine, terminal: TermId, text: &str) {
    engine
        .dispatch(Command::SendInput {
            terminal,
            text: text.to_owned(),
        })
        .await
        .expect("typed");
}

/// Waits until `check` holds for the screen, 20 s at most; the screen that held.
pub async fn until_screen(
    engine: &Engine,
    terminal: TermId,
    what: &str,
    check: impl Fn(&[String]) -> bool,
) -> Vec<String> {
    let started = Instant::now();
    loop {
        let lines = screen(engine, terminal).await;
        if check(&lines) {
            return lines;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "never {what}:\n{}",
            lines.join("\n")
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Types `echo <word>-$((6*7))` and waits for `<word>-42`, which only the shell's answer holds
/// (the echoed command line holds the sum unworked): the shell is back and has read every key
/// before it.
pub async fn shell_answers(engine: &Engine, terminal: TermId, word: &str) {
    type_in(engine, terminal, &format!("echo {word}-$((6*7))\r")).await;
    let answer = format!("{word}-42");
    until_screen(engine, terminal, &answer, |lines| {
        lines.iter().any(|line| line.trim() == answer)
    })
    .await;
}

fn contains(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|line| line.contains(needle))
}

/// vim's empty lines, `~` alone on a row.
fn tildes(lines: &[String]) -> usize {
    lines.iter().filter(|line| line.trim_end() == "~").count()
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn vim_draws_takes_keys_writes_its_file_and_gives_the_shell_back() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let mine = live.remote_dir("a", "t-vim");
    let terminal = open(&engine, id, 30, 100).await;
    shell_answers(&engine, terminal, "READY").await;

    type_in(
        &engine,
        terminal,
        &format!("vim -u NONE -N {mine}/note.txt\r"),
    )
    .await;
    let drawn = until_screen(&engine, terminal, "vim's screen", |lines| {
        tildes(lines) >= 20 && lines.last().is_some_and(|line| line.contains("note.txt"))
    })
    .await;
    eprintln!("vim:\n{}", drawn.join("\n"));
    type_in(&engine, terminal, "ilive vim ok").await;
    until_screen(&engine, terminal, "insert mode", |lines| {
        contains(lines, "-- INSERT --") && lines[0].trim_end() == "live vim ok"
    })
    .await;
    type_in(&engine, terminal, "\x1b").await;
    until_screen(&engine, terminal, "normal mode", |lines| {
        !contains(lines, "-- INSERT --")
    })
    .await;
    type_in(&engine, terminal, ":wq\r").await;
    // vim flushes the input it has not read when it gives the terminal back: wait for the shell.
    until_screen(&engine, terminal, "the shell's screen back", |lines| {
        tildes(lines) == 0 && lines.iter().any(|line| line.trim_end() == "a:~$")
    })
    .await;
    shell_answers(&engine, terminal, "DONE").await;
    assert_eq!(
        live.ctl(&["exec", "a", &format!("cat '{mine}/note.txt'")]),
        "live vim ok\n",
        "vim wrote what was typed"
    );
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn htop_draws_follows_a_new_size_and_quits() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let terminal = open(&engine, id, 30, 100).await;
    shell_answers(&engine, terminal, "READY").await;

    type_in(&engine, terminal, "htop\r").await;
    let drawn = until_screen(&engine, terminal, "htop's screen", |lines| {
        contains(lines, "Mem[")
            && contains(lines, "Tasks:")
            && lines
                .last()
                .is_some_and(|line| line.contains("F10") && line.contains("Quit"))
    })
    .await;
    eprintln!("htop at 30x100:\n{}", drawn.join("\n"));
    assert_eq!(drawn.len(), 30);
    assert!(drawn.iter().all(|line| line.chars().count() <= 100));

    // As the panel showing it does when it changes size.
    engine
        .terminal_session(terminal)
        .expect("the session")
        .resize(TermSize {
            rows: 40,
            cols: 140,
        })
        .expect("resized");
    let redrawn = until_screen(&engine, terminal, "htop at the new size", |lines| {
        lines.len() == 40
            && lines.last().is_some_and(|line| line.contains("F10"))
            && lines
                .iter()
                .any(|line| line.trim_end().chars().count() > 100)
    })
    .await;
    eprintln!("htop at 40x140:\n{}", redrawn.join("\n"));

    type_in(&engine, terminal, "q").await;
    until_screen(&engine, terminal, "the shell's screen back", |lines| {
        !contains(lines, "F10") && lines.iter().any(|line| line.trim_end() == "a:~$")
    })
    .await;
    shell_answers(&engine, terminal, "BACK").await;
    type_in(&engine, terminal, "stty size\r").await;
    until_screen(&engine, terminal, "the size the server knows", |lines| {
        lines.iter().any(|line| line.trim() == "40 140")
    })
    .await;
}
