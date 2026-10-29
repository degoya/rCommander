//! An in-process SSH server with a pretend shell, for the terminal tests (RC-060-03): the PTY
//! request, `window-change`, the end of a shell and a `sudo -i` that asks for a password.
//!
//! It takes [`super::PASSWORD`] for the login and for `sudo`. A shell channel echoes what it is
//! typed, like a terminal in its normal mode, except while `sudo` asks for the password; Enter
//! runs the line: `sudo -i` asks for the password and, given [`super::PASSWORD`], turns the prompt
//! into a root one; `whoami` and `pwd` answer; `cd <word>` reads the word as a POSIX shell does
//! (quotes, backslashes), so a test sees what a quoted path arrives as; `exit [n]` ends the shell
//! with that status; `hangup` closes the channel without one; `drop` ends the whole connection.
//! Every line it ran is recorded ([`ShellServer::lines`]), a password as `<password>`.
//! [`ShellServer::freeze`] makes the connections accepted so far go silent, as a paused or
//! unplugged server does (RC-080-04).

use std::{
    mem,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use russh::{
    Channel, ChannelId, Disconnect, MethodKind, MethodSet,
    keys::PrivateKey,
    server::{self, Auth, ChannelOpenHandle, Msg, Session},
};
use tokio::{net::TcpListener, task::JoinHandle};

use super::{PASSWORD, server::link::Link};

/// What `sudo -i` prints before it waits for the password.
pub const SUDO_PROMPT: &str = "[sudo] password for tester: ";

/// A running shell server; stopped when dropped.
pub struct ShellServer {
    pub port: u16,
    seen: Arc<ShellSeen>,
    /// The switch of each connection accepted so far that silences it.
    links: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
    task: JoinHandle<()>,
}

/// A PTY request as the server received it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyRequest {
    pub term: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Default)]
struct ShellSeen {
    accepts: AtomicUsize,
    ptys: Mutex<Vec<PtyRequest>>,
    /// `(cols, rows)` of each `window-change`.
    resizes: Mutex<Vec<(u32, u32)>>,
    lines: Mutex<Vec<String>>,
    /// The folder of the last `cd`.
    cwd: Mutex<Option<String>>,
}

impl ShellServer {
    /// Starts a server with `host_key` on a free port of 127.0.0.1.
    ///
    /// # Panics
    ///
    /// No port can be bound.
    pub async fn start(host_key: PrivateKey) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let config = Arc::new(server::Config {
            keys: vec![host_key],
            methods: MethodSet::from([MethodKind::Password].as_slice()),
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..server::Config::default()
        });
        let seen = Arc::new(ShellSeen::default());
        let recorded = Arc::clone(&seen);
        let links: Arc<Mutex<Vec<Arc<AtomicBool>>>> = Arc::default();
        let switches = Arc::clone(&links);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                recorded.accepts.fetch_add(1, Ordering::SeqCst);
                let frozen = Arc::new(AtomicBool::new(false));
                lock(&switches).push(Arc::clone(&frozen));
                let link = Link { stream, frozen };
                let handler = Shell {
                    seen: Arc::clone(&recorded),
                    line: Vec::new(),
                    asking: false,
                    root: false,
                };
                let config = Arc::clone(&config);
                tokio::spawn(async move {
                    if let Ok(running) = server::run_stream(config, link, handler).await {
                        let _ = running.await;
                    }
                });
            }
        });
        Self {
            port,
            seen,
            links,
            task,
        }
    }

    /// Every connection accepted so far stops answering and swallows what it is sent.
    pub fn freeze(&self) {
        for link in lock(&self.links).iter() {
            link.store(true, Ordering::SeqCst);
        }
    }

    /// The TCP connections accepted so far.
    #[must_use]
    pub fn accepts(&self) -> usize {
        self.seen.accepts.load(Ordering::SeqCst)
    }

    /// The PTY requests received so far.
    #[must_use]
    pub fn ptys(&self) -> Vec<PtyRequest> {
        lock(&self.seen.ptys).clone()
    }

    /// The sizes of the `window-change` requests received so far, as `(cols, rows)`.
    #[must_use]
    pub fn resizes(&self) -> Vec<(u32, u32)> {
        lock(&self.seen.resizes).clone()
    }

    /// The lines the shells ran so far; a password typed for `sudo` is `<password>`.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        lock(&self.seen.lines).clone()
    }

    /// The folder the last `cd` went to, as the shell read its word.
    #[must_use]
    pub fn cwd(&self) -> Option<String> {
        lock(&self.seen.cwd).clone()
    }
}

impl Drop for ShellServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// One connection's pretend shell. Every shell channel of the connection shares it.
struct Shell {
    seen: Arc<ShellSeen>,
    line: Vec<u8>,
    /// `sudo` waits for the password.
    asking: bool,
    root: bool,
}

impl Shell {
    fn prompt(&self) -> &'static str {
        if self.root {
            "root@test:~# "
        } else {
            "tester@test:~$ "
        }
    }

    /// Runs one line; the shell's answer and its next prompt follow.
    fn run(&mut self, channel: ChannelId, line: &str, session: &mut Session) -> Result<(), Error> {
        if mem::take(&mut self.asking) {
            lock(&self.seen.lines).push("<password>".to_owned());
            let answer = if line == PASSWORD {
                self.root = true;
                "\r\n".to_owned()
            } else {
                "\r\nSorry, try again.\r\n".to_owned()
            };
            session.data(channel, answer.into_bytes())?;
            return session.data(channel, self.prompt().as_bytes().to_vec());
        }
        lock(&self.seen.lines).push(line.to_owned());
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        let answer = match command {
            "sudo" if rest == "-i" => {
                self.asking = true;
                return session.data(channel, format!("\r\n{SUDO_PROMPT}").into_bytes());
            }
            "exit" => {
                session.data(channel, b"\r\nlogout\r\n".to_vec())?;
                session.exit_status_request(channel, rest.parse().unwrap_or(0))?;
                session.eof(channel)?;
                return session.close(channel);
            }
            "hangup" => return session.close(channel),
            "drop" => return session.disconnect(Disconnect::ByApplication, "bye", "en"),
            "whoami" => format!("\r\n{}", if self.root { "root" } else { "tester" }),
            "pwd" => format!("\r\n{}", lock(&self.seen.cwd).as_deref().unwrap_or("~")),
            "cd" => {
                *lock(&self.seen.cwd) = Some(shell_word(rest));
                String::new()
            }
            _ => String::new(),
        };
        session.data(
            channel,
            format!("{answer}\r\n{}", self.prompt()).into_bytes(),
        )
    }
}

type Error = russh::Error;

impl server::Handler for Shell {
    type Error = Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        lock(&self.seen.ptys).push(PtyRequest {
            term: term.to_owned(),
            cols,
            rows,
        });
        session.channel_success(channel)
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        lock(&self.seen.resizes).push((cols, rows));
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let greeting = format!("Welcome to the test shell\r\n{}", self.prompt());
        session.data(channel, greeting.into_bytes())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        for &byte in data {
            match byte {
                b'\r' | b'\n' => {
                    let line = String::from_utf8_lossy(&mem::take(&mut self.line)).into_owned();
                    self.run(channel, &line, session)?;
                }
                0x7f => {
                    self.line.pop();
                }
                _ => {
                    self.line.push(byte);
                    if !self.asking {
                        session.data(channel, vec![byte])?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// `text` read as one word of a POSIX shell: single quotes keep everything up to the next one,
/// double quotes everything but a backslash before `"`, `\`, `$` or a backquote, a backslash
/// outside quotes the next character.
#[must_use]
pub fn shell_word(text: &str) -> String {
    let mut word = String::new();
    let mut chars = text.chars();
    while let Some(next) = chars.next() {
        match next {
            '\'' => word.extend(chars.by_ref().take_while(|&c| c != '\'')),
            '"' => {
                while let Some(quoted) = chars.next() {
                    match quoted {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => word.push(escaped),
                            Some(other) => word.extend(['\\', other]),
                            None => word.push('\\'),
                        },
                        other => word.push(other),
                    }
                }
            }
            '\\' => word.extend(chars.next()),
            other => word.push(other),
        }
    }
    word
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
