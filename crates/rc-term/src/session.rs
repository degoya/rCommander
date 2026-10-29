//! A terminal session: a back end, the `alacritty_terminal` [`Term`] its output feeds, and what
//! the app shows about it (id, title, folder, size, state).
//!
//! One task per session (the pump) moves the back end's output through the VT parser into the
//! [`Term`] and answers what the [`Term`] asks to send back (cursor position reports, device
//! attributes) through the back end. Viewers learn about changes from [`TermSession::subscribe`]
//! and read the [`Term`] themselves: the panel widget renders it, MCP reads it as text.
//! On the way the pump looks for what the shell says about its folder and prompt (`osc.rs`,
//! `cwd_sync.rs`, RC-040-04).

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    sync::FairMutex,
    term::Config,
    vte::ansi::Processor,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::JoinHandle,
};

use crate::{
    BackendEvent, ScreenText, Spawned, TermBackend, TermError, TermSize,
    cwd_sync::{Changes, CwdSync},
    osc::{OscScanner, local_host_names},
    screen_text,
};

/// Scrollback lines a session keeps unless told otherwise.
pub const DEFAULT_SCROLLBACK: usize = 10_000;

/// How many [`TermEvent`]s a slow viewer may fall behind before it misses some; it then
/// re-reads the [`Term`], which is all a missed event would have told it to do.
const EVENT_BUFFER: usize = 256;

/// A session's id, unique in the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TermId(pub u64);

impl TermId {
    /// A fresh id.
    #[must_use]
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl fmt::Display for TermId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Whether the program still runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SessionState {
    Running,
    /// The program ended, with its exit code when one is known.
    Exited {
        code: Option<u32>,
    },
}

impl SessionState {
    /// Whether the program has ended.
    #[must_use]
    pub fn is_exited(self) -> bool {
        matches!(self, Self::Exited { .. })
    }
}

/// What changed in a session. A viewer re-reads the [`Term`] or the session after it.
#[derive(Debug, Clone)]
pub enum TermEvent {
    /// The screen changed.
    Wakeup,
    /// The program set a new title.
    Title(String),
    /// The program ended.
    Exited(Option<u32>),
    /// Something only a window can answer or show: the bell, the clipboard, a colour or size
    /// request, the cursor's blinking.
    Ui(Event),
    /// The shell reported another folder ([`TermSession::reported_folder`]).
    Folder,
    /// The prompt state or the follow status changed ([`TermSession::prompt_state`],
    /// [`TermSession::follow_status`]).
    Prompt,
}

/// How a session starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// The title until the program sets one.
    pub title: String,
    /// The id of the shell profile (or back end) it runs, for lists and the GUI.
    pub shell: String,
    /// The folder the program starts in. The session keeps it as its folder until the shell
    /// reports another one or [`TermSession::follow`] sends it elsewhere (RC-040-04).
    pub cwd: PathBuf,
    pub size: TermSize,
    /// Lines kept above the screen.
    pub scrollback: usize,
}

/// A session as lists and MCP show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: TermId,
    pub title: String,
    pub shell: String,
    pub cwd: PathBuf,
    pub size: TermSize,
    pub state: SessionState,
}

/// The [`Term`]'s link back to its session: what the terminal wants done goes to the pump.
#[derive(Clone)]
pub struct EventProxy(mpsc::UnboundedSender<Event>);

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        // The pump is gone only when the session is; nothing is left to tell then.
        let _ = self.0.send(event);
    }
}

/// A running (or ended) terminal session. Dropping it ends the program.
pub struct TermSession {
    id: TermId,
    shell: String,
    shared: Arc<Shared>,
    backend: Arc<dyn TermBackend>,
    pump: JoinHandle<()>,
    /// What typed input goes to while it is held ([`TermSession::hold_input`]).
    held: Mutex<Option<InputHook>>,
}

/// Takes what is typed while a session's input is held.
pub type InputHook = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// What the session and its pump share.
struct Shared {
    term: Arc<FairMutex<Term<EventProxy>>>,
    default_title: String,
    title: Mutex<String>,
    size: Mutex<TermSize>,
    state: watch::Sender<SessionState>,
    events: broadcast::Sender<TermEvent>,
    /// The folder and the prompt; held while the app or the user types (`cwd_sync.rs`).
    sync: Mutex<CwdSync>,
}

impl TermSession {
    /// A session over a started back end. Runs its pump on the current tokio runtime.
    ///
    /// # Panics
    ///
    /// Outside a tokio runtime.
    #[must_use]
    pub fn start(id: TermId, spawned: Spawned, options: SessionOptions) -> Self {
        let (replies, replies_received) = mpsc::unbounded_channel();
        let config = Config {
            scrolling_history: options.scrollback,
            ..Config::default()
        };
        let term = Term::new(config, &options.size, EventProxy(replies));
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let shared = Arc::new(Shared {
            term: Arc::new(FairMutex::new(term)),
            default_title: options.title.clone(),
            title: Mutex::new(options.title),
            size: Mutex::new(options.size),
            state: watch::Sender::new(SessionState::Running),
            events,
            sync: Mutex::new(CwdSync::new(options.cwd, local_host_names().to_vec())),
        });
        let backend: Arc<dyn TermBackend> = Arc::from(spawned.backend);
        let pump = tokio::spawn(pump(
            Arc::clone(&shared),
            Arc::clone(&backend),
            spawned.events,
            replies_received,
        ));
        Self {
            id,
            shell: options.shell,
            shared,
            backend,
            pump,
            held: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn id(&self) -> TermId {
        self.id
    }

    /// The program's title, or the program's name until it sets one.
    #[must_use]
    pub fn title(&self) -> String {
        lock(&self.shared.title).clone()
    }

    /// The shell profile (or back end) id it was started with.
    #[must_use]
    pub fn shell(&self) -> &str {
        &self.shell
    }

    /// The session's folder: the one the shell last reported, else where it started or where
    /// [`Self::set_cwd`] or [`Self::follow`] last put it.
    #[must_use]
    pub fn cwd(&self) -> PathBuf {
        self.sync().current()
    }

    /// Records the session's folder; sends nothing to the program.
    pub fn set_cwd(&self, cwd: &Path) {
        self.sync().set_folder(cwd);
    }

    #[must_use]
    pub fn size(&self) -> TermSize {
        *lock(&self.shared.size)
    }

    #[must_use]
    pub fn state(&self) -> SessionState {
        *self.shared.state.borrow()
    }

    /// The process id of a local program.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.backend.process_id()
    }

    /// Everything a list shows.
    #[must_use]
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id,
            title: self.title(),
            shell: self.shell.clone(),
            cwd: self.cwd(),
            size: self.size(),
            state: self.state(),
        }
    }

    /// Sends `bytes` to the program as if typed (`\r` is Enter).
    ///
    /// # Errors
    ///
    /// [`TermError::Exited`] once the program has ended; [`TermError::Busy`] while the input is
    /// held, after handing the bytes to the hold's hook.
    pub fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        if self.state().is_exited() {
            return Err(TermError::Exited);
        }
        let hook = lock(&self.held).clone();
        if let Some(hook) = hook {
            hook(&bytes);
            return Err(TermError::Busy);
        }
        // Held while written: the follow of the panel sees every key in order.
        let mut sync = self.sync();
        sync.on_input(&bytes);
        self.backend.write(bytes)
    }

    /// Gives the terminal and the program a new size.
    ///
    /// # Errors
    ///
    /// [`TermError::Exited`] once the program has ended, or the back end's own failure.
    pub fn resize(&self, size: TermSize) -> Result<(), TermError> {
        if self.state().is_exited() {
            return Err(TermError::Exited);
        }
        self.backend.resize(size)?;
        self.shared.term.lock().resize(size);
        *lock(&self.shared.size) = size;
        self.shared.notify(TermEvent::Wakeup);
        Ok(())
    }

    /// The live screen and up to `scrollback` lines above it, as text.
    #[must_use]
    pub fn screen(&self, scrollback: usize) -> ScreenText {
        screen_text(&*self.shared.term.lock(), scrollback)
    }

    /// The emulated terminal, for a widget that renders it.
    #[must_use]
    pub fn term(&self) -> Arc<FairMutex<Term<EventProxy>>> {
        Arc::clone(&self.shared.term)
    }

    /// Changes from now on.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<TermEvent> {
        self.shared.events.subscribe()
    }

    /// Ends the program; [`Self::wait_exit`] then returns.
    pub fn shutdown(&self) {
        self.backend.shutdown();
    }

    /// Waits until the program has ended and its output is in the terminal; its exit code.
    pub async fn wait_exit(&self) -> Option<u32> {
        let mut state = self.shared.state.subscribe();
        match state.wait_for(|state| state.is_exited()).await {
            Ok(state) => match *state {
                SessionState::Exited { code } => code,
                SessionState::Running => None,
            },
            // The sender lives in `self`; it cannot be gone while we wait.
            Err(_) => None,
        }
    }

    /// Whether the pump has stopped: after the end, nothing of the session runs any more.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.pump.is_finished()
    }

    pub(crate) fn sync(&self) -> MutexGuard<'_, CwdSync> {
        lock(&self.shared.sync)
    }

    pub(crate) fn backend(&self) -> &dyn TermBackend {
        &*self.backend
    }
}

impl Drop for TermSession {
    fn drop(&mut self) {
        self.backend.shutdown();
        self.pump.abort();
    }
}

impl fmt::Debug for TermSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TermSession")
            .field("id", &self.id)
            .field("shell", &self.shell)
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn notify(&self, event: TermEvent) {
        // No viewer is not an error: a headless session has none.
        let _ = self.events.send(event);
    }

    fn set_title(&self, title: String) {
        *lock(&self.title) = title.clone();
        self.notify(TermEvent::Title(title));
    }

    /// Reads the shell's OSC payloads in `bytes`; a `cd` waiting for the prompt is written
    /// while the state is held. Output without an ESC costs one scan.
    fn scan(&self, backend: &dyn TermBackend, scanner: &mut OscScanner, bytes: &[u8]) {
        if scanner.is_idle() && !bytes.contains(&0x1b) {
            return;
        }
        let mut changes = Changes::default();
        {
            let mut sync = lock(&self.sync);
            scanner.feed(bytes, |payload| sync.on_payload(payload, &mut changes));
            if let Some(line) = changes.write.take() {
                let _ = backend.write(line);
            }
        }
        if changes.folder {
            self.notify(TermEvent::Folder);
        }
        if changes.prompt {
            self.notify(TermEvent::Prompt);
        }
    }

    /// What the terminal asked for while parsing.
    fn answer(&self, backend: &dyn TermBackend, event: Event) {
        match event {
            Event::PtyWrite(reply) => {
                let _ = backend.write(reply.into_bytes());
            }
            Event::Title(title) => self.set_title(title),
            Event::ResetTitle => self.set_title(self.default_title.clone()),
            // The pump reports the end from the back end; alacritty's own loop is not used.
            Event::Wakeup | Event::Exit | Event::ChildExit(_) => {}
            other => self.notify(TermEvent::Ui(other)),
        }
    }
}

/// Moves output into the terminal and the terminal's replies to the program, until the end.
async fn pump(
    shared: Arc<Shared>,
    backend: Arc<dyn TermBackend>,
    mut output: mpsc::Receiver<BackendEvent>,
    mut replies: mpsc::UnboundedReceiver<Event>,
) {
    let mut parser: Processor = Processor::new();
    let mut scanner = OscScanner::default();
    let code = loop {
        let sync_deadline = parser.sync_timeout().sync_timeout();
        tokio::select! {
            biased;
            Some(event) = replies.recv() => shared.answer(&*backend, event),
            event = output.recv() => match event {
                Some(BackendEvent::Output(bytes)) => {
                    shared.scan(&*backend, &mut scanner, &bytes);
                    parser.advance(&mut *shared.term.lock(), &bytes);
                    shared.notify(TermEvent::Wakeup);
                }
                Some(BackendEvent::Exited(code)) => break code,
                None => break None,
            },
            () = until(sync_deadline) => {
                // A synchronized update the program never closed: show what it drew.
                parser.stop_sync(&mut *shared.term.lock());
                shared.notify(TermEvent::Wakeup);
            }
        }
    };
    shared.state.send_replace(SessionState::Exited { code });
    shared.notify(TermEvent::Exited(code));
}

/// Sleeps until `deadline`, or forever without one.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending().await,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

mod hold;

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
