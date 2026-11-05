//! The local back end: a program in a pseudo-terminal of this machine, through `portable-pty`
//! (ConPTY on Windows, openpty elsewhere).
//!
//! Three threads per program, because a PTY is blocking IO: one reads the output into the event
//! channel, one writes queued input, one waits for the process. The waiter reaps the process (no
//! zombie), then drops the PTY's master side - on Windows that closes the pseudo console, which
//! is what ends the reader there - and gives the reader [`DRAIN_GRACE`] to hand over the last
//! output before it reports [`BackendEvent::Exited`]. A reader still blocked after that (a
//! background job holds the PTY open) ends with that job or when the session drops the channel.
//!
//! Ending: on Windows the program is in a job object of its own (`job.rs`, RC-040-06), so a
//! closed terminal ends it and what it started, and rCommander's end - however it comes - ends
//! them too; [`LocalCommand::end_programs_on_close`] says whether what it started is in the job.
//! Elsewhere the close hangs the program up (`SIGHUP`); one that ignores it gets `SIGTERM` and
//! then `SIGKILL`, each after [`ending::END_GRACE`] (`ending.rs`, `hangup.rs`, RC-080-62).
//!
//! Console window: a program started into a pseudo console never gets a window of its own, so
//! the `NoConsoleWindow` flag of `rc-process` (which only applies to `std`/`tokio` `Command`) is
//! not needed; `portable-pty` starts it with `CreateProcessW` and the ConPTY attribute.

use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc as std_mpsc,
    },
    thread,
    time::Duration,
};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::recover::LockOrRecover as _;
use crate::{BackendEvent, INPUT_QUEUE, Spawned, TermBackend, TermError, TermSize};

mod ending;
#[cfg(unix)]
mod hangup;
#[cfg(windows)]
mod job;
#[cfg(not(windows))]
mod job {
    //! Elsewhere no job: the PTY's end hangs the program up, and what it started with it.

    pub(super) struct Job;

    impl Job {
        pub(super) fn of(
            _child: &(dyn portable_pty::Child + Send + Sync),
            _whole_tree: bool,
        ) -> Option<Self> {
            None
        }

        pub(super) fn kill(&self) {}
    }
}

/// Output chunks in flight between the reader thread and the session before the reader waits.
const OUTPUT_QUEUE: usize = 64;

/// One read from the PTY at most.
const READ_CHUNK: usize = 16 * 1024;

/// How long the waiter lets the reader deliver the output written just before the exit.
const DRAIN_GRACE: Duration = Duration::from_millis(1000);

/// A local program to start in a PTY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCommand {
    /// The program: an absolute path, or a name looked up on `PATH`.
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The folder it starts in; the user's home when `None`.
    pub cwd: Option<PathBuf>,
    /// Variables added to the inherited environment (after [`default_env`]).
    pub env: Vec<(String, String)>,
    /// Whether what the program starts ends with the terminal and with rCommander too (the
    /// setting `terminal.end_programs_on_close`, on by default), or only the program does
    /// (Windows, RC-040-06). Each start carries its own (RC-080-51).
    pub end_programs_on_close: bool,
}

/// The variables every local terminal sets: a terminal type the emulation answers to, on Unix.
#[must_use]
pub fn default_env() -> Vec<(String, String)> {
    if cfg!(windows) {
        Vec::new()
    } else {
        vec![
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]
    }
}

/// Starts `command` in a new PTY of `size`.
///
/// # Errors
///
/// [`TermError::Pty`] when the PTY cannot be opened, the program not started or a thread not
/// spawned; for a program named by a full path that is not there, with that path.
pub fn spawn_local(command: &LocalCommand, size: TermSize) -> Result<Spawned, TermError> {
    // A system program missing from System32 (RC-080-65) says where it was looked for.
    if command.program.is_absolute() && !command.program.is_file() {
        return Err(TermError::Pty(format!(
            "{} was not found",
            command.program.display()
        )));
    }
    let pair = native_pty_system()
        .openpty(pty_size(size))
        .map_err(pty_error)?;
    let mut builder = CommandBuilder::new(&command.program);
    builder.args(&command.args);
    if let Some(cwd) = &command.cwd {
        builder.cwd(cwd);
    }
    for (key, value) in default_env().iter().chain(&command.env) {
        builder.env(key, value);
    }
    let mut child = pair.slave.spawn_command(builder).map_err(pty_error)?;
    // The child holds the only other slave handle now; its exit is the reader's end of file.
    drop(pair.slave);
    let process_id = child.process_id();
    // By the process handle, never the id: an id may name another process once this one ended.
    let job = job::Job::of(&*child, command.end_programs_on_close);
    let life = Arc::new(ending::Life::default());
    let killer = Killer {
        killer: Mutex::new(child.clone_killer()),
        life: Arc::clone(&life),
    };
    let started = start_threads(pair.master, &life, move || child.wait());
    match started {
        Ok((input, master, events)) => Ok(Spawned {
            backend: Box::new(LocalPty {
                input: Mutex::new(Some(input)),
                master,
                killer,
                job,
                process_id,
                end_programs_on_close: command.end_programs_on_close,
                closing: AtomicBool::new(false),
            }),
            events,
        }),
        Err(error) => {
            killer.kill();
            Err(error)
        }
    }
}

type SharedMaster = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

/// The input sender, the shared master and the event receiver [`start_threads`] hands back.
type Threads = (
    std_mpsc::SyncSender<Vec<u8>>,
    SharedMaster,
    mpsc::Receiver<BackendEvent>,
);

/// The reader, writer and waiter threads; what the back end keeps of them.
fn start_threads(
    master: Box<dyn MasterPty + Send>,
    life: &Arc<ending::Life>,
    wait: impl FnOnce() -> std::io::Result<portable_pty::ExitStatus> + Send + 'static,
) -> Result<Threads, TermError> {
    let reader = master.try_clone_reader().map_err(pty_error)?;
    let writer = master.take_writer().map_err(pty_error)?;
    let master: SharedMaster = Arc::new(Mutex::new(Some(master)));
    let (events, receiver) = mpsc::channel(OUTPUT_QUEUE);
    let (input, queued) = std_mpsc::sync_channel::<Vec<u8>>(INPUT_QUEUE);

    let output = events.clone();
    let reader_life = Arc::clone(life);
    spawn_thread("rc-term-read", move || {
        read_loop(reader, &output);
        reader_life.mark_drained();
    })?;
    spawn_thread("rc-term-write", move || write_loop(writer, &queued))?;
    let waiter_master = Arc::clone(&master);
    let waiter_life = Arc::clone(life);
    spawn_thread("rc-term-wait", move || {
        let code = match wait() {
            Ok(status) => Some(status.exit_code()),
            Err(error) => {
                tracing::warn!(%error, "waiting for the terminal's program failed");
                None
            }
        };
        waiter_life.mark_exited();
        // Closes the pseudo console on Windows, so the reader sees its end of file there.
        drop(lock(&waiter_master).take());
        if !waiter_life.wait_drained(DRAIN_GRACE) {
            tracing::debug!("the terminal's output is still held open after its exit");
        }
        let _ = events.blocking_send(BackendEvent::Exited(code));
    })?;
    Ok((input, master, receiver))
}

fn read_loop(mut reader: Box<dyn Read + Send>, output: &mpsc::Sender<BackendEvent>) {
    let mut buffer = vec![0_u8; READ_CHUNK];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let chunk = buffer.get(..read).unwrap_or_default().to_vec();
                if output.blocking_send(BackendEvent::Output(chunk)).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn write_loop(mut writer: Box<dyn Write + Send>, queued: &std_mpsc::Receiver<Vec<u8>>) {
    while let Ok(bytes) = queued.recv() {
        if writer
            .write_all(&bytes)
            .and_then(|()| writer.flush())
            .is_err()
        {
            break;
        }
    }
}

/// Queues `bytes` for the writer thread, never waiting for room (RC-080-65).
fn queue(input: &std_mpsc::SyncSender<Vec<u8>>, bytes: Vec<u8>) -> Result<(), TermError> {
    input.try_send(bytes).map_err(|error| match error {
        std_mpsc::TrySendError::Full(_) => TermError::InputFull,
        std_mpsc::TrySendError::Disconnected(_) => TermError::Exited,
    })
}

fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), TermError> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
        .map_err(|error| TermError::Pty(format!("cannot start a thread: {error}")))
}

/// Kills the process unless the waiter has reaped it: after the reap its id may belong to
/// another process.
struct Killer {
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    life: Arc<ending::Life>,
}

impl Killer {
    fn kill(&self) {
        self.life.unless_exited(|| {
            let _ = lock(&self.killer).kill();
        });
    }
}

/// The back end of a local program.
struct LocalPty {
    /// `None` after [`TermBackend::shutdown`].
    input: Mutex<Option<std_mpsc::SyncSender<Vec<u8>>>>,
    /// `None` once the program has ended.
    master: SharedMaster,
    killer: Killer,
    /// The program's job object (Windows): ended with the terminal, closed with rCommander.
    job: Option<job::Job>,
    process_id: Option<u32>,
    /// `terminal.end_programs_on_close`: on Unix whether the escalation after the hang-up goes
    /// to the program's groups or to the program alone (`hangup.rs`).
    #[cfg_attr(not(unix), allow(dead_code, reason = "read by the Unix escalation"))]
    end_programs_on_close: bool,
    /// Set by the first [`TermBackend::shutdown`]; a second one starts nothing.
    closing: AtomicBool,
}

impl TermBackend for LocalPty {
    fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        if lock(&self.master).is_none() {
            return Err(TermError::Exited);
        }
        match lock(&self.input).as_ref() {
            Some(input) => queue(input, bytes),
            None => Err(TermError::Exited),
        }
    }

    fn resize(&self, size: TermSize) -> Result<(), TermError> {
        match lock(&self.master).as_ref() {
            Some(master) => master.resize(pty_size(size)).map_err(pty_error),
            None => Err(TermError::Exited),
        }
    }

    fn shutdown(&self) {
        // Ends the writer thread too.
        drop(lock(&self.input).take());
        self.killer.kill();
        // What the program started, and the program if the killer no longer may.
        if let Some(job) = &self.job {
            job.kill();
        }
        if !self.closing.swap(true, Ordering::SeqCst) {
            self.after_hangup();
        }
    }

    fn process_id(&self) -> Option<u32> {
        self.process_id
    }
}

impl LocalPty {
    /// Unix: lets the master go now rather than with the waiter, and starts the escalation for
    /// a program that still runs (RC-080-62).
    #[cfg(unix)]
    fn after_hangup(&self) {
        let master = lock(&self.master).take();
        // Read before the master goes: the group an interactive shell runs its program in.
        let foreground = master
            .as_ref()
            .and_then(|master| master.process_group_leader());
        drop(master);
        if let Some(program) = self.process_id {
            self.killer.life.unless_exited(|| {
                hangup::after_hangup(
                    &self.killer.life,
                    program,
                    foreground,
                    self.end_programs_on_close,
                );
            });
        }
    }

    /// Windows: the job object has ended the program and what it started.
    #[cfg(not(unix))]
    fn after_hangup(&self) {}
}

impl Drop for LocalPty {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn pty_size(size: TermSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn pty_error(error: impl std::fmt::Display) -> TermError {
    TermError::Pty(error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock_or_recover("local_pty.mutex")
}

#[cfg(all(test, unix))]
#[path = "local_pty_tests.rs"]
mod tests;
