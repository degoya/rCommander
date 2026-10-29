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
//! Elsewhere the PTY's end hangs the program up.
//!
//! Console window: a program started into a pseudo console never gets a window of its own, so
//! the `NoConsoleWindow` flag of `rc-process` (which only applies to `std`/`tokio` `Command`) is
//! not needed; `portable-pty` starts it with `CreateProcessW` and the ConPTY attribute.

use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{self as std_mpsc, RecvTimeoutError},
    },
    thread,
    time::Duration,
};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::{BackendEvent, Spawned, TermBackend, TermError, TermSize};

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
/// spawned.
pub fn spawn_local(command: &LocalCommand, size: TermSize) -> Result<Spawned, TermError> {
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
    let killer = Arc::new(Killer {
        killer: Mutex::new(child.clone_killer()),
        exited: Mutex::new(false),
    });
    let started = start_threads(pair.master, &killer, move || child.wait());
    match started {
        Ok((input, master, events)) => Ok(Spawned {
            backend: Box::new(LocalPty {
                input: Mutex::new(Some(input)),
                master,
                killer,
                job,
                process_id,
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
    std_mpsc::Sender<Vec<u8>>,
    SharedMaster,
    mpsc::Receiver<BackendEvent>,
);

/// The reader, writer and waiter threads; what the back end keeps of them.
fn start_threads(
    master: Box<dyn MasterPty + Send>,
    killer: &Arc<Killer>,
    wait: impl FnOnce() -> std::io::Result<portable_pty::ExitStatus> + Send + 'static,
) -> Result<Threads, TermError> {
    let reader = master.try_clone_reader().map_err(pty_error)?;
    let writer = master.take_writer().map_err(pty_error)?;
    let master: SharedMaster = Arc::new(Mutex::new(Some(master)));
    let (events, receiver) = mpsc::channel(OUTPUT_QUEUE);
    let (input, queued) = std_mpsc::channel::<Vec<u8>>();
    let (drained, drained_signal) = std_mpsc::channel::<()>();

    let output = events.clone();
    spawn_thread("rc-term-read", move || read_loop(reader, &output, drained))?;
    spawn_thread("rc-term-write", move || write_loop(writer, &queued))?;
    let waiter_master = Arc::clone(&master);
    let waiter_killer = Arc::clone(killer);
    spawn_thread("rc-term-wait", move || {
        let code = match wait() {
            Ok(status) => Some(status.exit_code()),
            Err(error) => {
                tracing::warn!(%error, "waiting for the terminal's program failed");
                None
            }
        };
        waiter_killer.mark_exited();
        // Closes the pseudo console on Windows, so the reader sees its end of file there.
        drop(lock(&waiter_master).take());
        match drained_signal.recv_timeout(DRAIN_GRACE) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
            Err(RecvTimeoutError::Timeout) => {
                tracing::debug!("the terminal's output is still held open after its exit");
            }
        }
        let _ = events.blocking_send(BackendEvent::Exited(code));
    })?;
    Ok((input, master, receiver))
}

fn read_loop(
    mut reader: Box<dyn Read + Send>,
    output: &mpsc::Sender<BackendEvent>,
    drained: std_mpsc::Sender<()>,
) {
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
    let _ = drained.send(());
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
    exited: Mutex<bool>,
}

impl Killer {
    fn kill(&self) {
        let exited = lock(&self.exited);
        if !*exited {
            let _ = lock(&self.killer).kill();
        }
    }

    fn mark_exited(&self) {
        *lock(&self.exited) = true;
    }
}

/// The back end of a local program.
struct LocalPty {
    /// `None` after [`TermBackend::shutdown`].
    input: Mutex<Option<std_mpsc::Sender<Vec<u8>>>>,
    /// `None` once the program has ended.
    master: SharedMaster,
    killer: Arc<Killer>,
    /// The program's job object (Windows): ended with the terminal, closed with rCommander.
    job: Option<job::Job>,
    process_id: Option<u32>,
}

impl TermBackend for LocalPty {
    fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        if lock(&self.master).is_none() {
            return Err(TermError::Exited);
        }
        match lock(&self.input).as_ref() {
            Some(input) => input.send(bytes).map_err(|_| TermError::Exited),
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
    }

    fn process_id(&self) -> Option<u32> {
        self.process_id
    }
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
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(all(test, unix))]
#[path = "local_pty_tests.rs"]
mod tests;
