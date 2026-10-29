//! An in-process SSH server for the session tests: no Docker, no network beyond 127.0.0.1.
//!
//! It accepts a password (also as the answer to a `keyboard-interactive` prompt) and the public
//! keys it is given, offers the methods it is told to, acts out a few `exec` commands
//! ([`super::exec`]; any other is echoed back), and echoes what a shell channel receives. Every
//! TCP connection it accepts is counted, so a test can prove that two channels share one, and
//! so is every authentication request it receives ([`TestServer::auth_attempts`]: no secret
//! before a trusted host key);
//! [`TestServer::freeze`] makes the connections accepted so far go silent, which to the client
//! looks like a dead network. Every forwarding request a client sends is recorded
//! ([`TestServer::forward_requests`]); with `probe_forwarding` the server tries to open agent,
//! X11, forwarded TCP/IP and session channels towards the client after the login and records
//! whether the client took them ([`TestServer::probes`]). With [`ServerOptions::sftp`] it serves
//! the `sftp` subsystem from a directory on disk (`super::sftp`).

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use russh::{
    Channel, ChannelId, MethodKind, MethodSet, Sig,
    keys::{PrivateKey, PublicKey},
    server::{self, Auth, ChannelOpenHandle, Msg, Response, Session},
};
use tokio::{net::TcpListener, task::JoinHandle};

use self::link::Link;
use super::exec::ExecRuns;

pub(super) mod link;

/// The one prompt of the `keyboard-interactive` method.
const PROMPTS: &[(Cow<'static, str>, bool)] = &[(Cow::Borrowed("Password: "), false)];

/// What the server accepts and offers.
#[derive(Clone)]
pub struct ServerOptions {
    pub host_key: PrivateKey,
    /// The password accepted for every user; none when `None`.
    pub password: Option<String>,
    /// The public keys accepted for every user.
    pub authorized: Vec<PublicKey>,
    /// The methods offered to the client.
    pub methods: Vec<MethodKind>,
    /// After the login, try to open channels towards the client ([`TestServer::probes`]).
    pub probe_forwarding: bool,
    /// The `sftp` subsystem; refused when `None`.
    pub sftp: Option<super::SftpServe>,
}

impl ServerOptions {
    /// A server with `host_key` that takes [`super::PASSWORD`] and the fixtures' client keys over
    /// `password`, `publickey` and `keyboard-interactive`.
    #[must_use]
    pub fn new(host_key: PrivateKey) -> Self {
        Self {
            host_key,
            password: Some(super::PASSWORD.to_owned()),
            authorized: super::client_public_keys(),
            methods: vec![
                MethodKind::Password,
                MethodKind::PublicKey,
                MethodKind::KeyboardInteractive,
            ],
            probe_forwarding: false,
            sftp: None,
        }
    }
}

/// A running server; stopped when dropped.
pub struct TestServer {
    pub port: u16,
    accepts: Arc<AtomicUsize>,
    auth_attempts: Arc<AtomicUsize>,
    seen: Arc<Seen>,
    links: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
    exec: Arc<ExecRuns>,
    task: JoinHandle<()>,
}

impl TestServer {
    /// Starts a server on a free port of 127.0.0.1.
    ///
    /// # Panics
    ///
    /// No port can be bound.
    pub async fn start(options: ServerOptions) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let accepts = Arc::new(AtomicUsize::new(0));
        let auth_attempts = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Seen::default());
        let links: Arc<Mutex<Vec<Arc<AtomicBool>>>> = Arc::default();
        let exec: Arc<ExecRuns> = Arc::default();
        let config = Arc::new(server::Config {
            keys: vec![options.host_key.clone()],
            methods: MethodSet::from(options.methods.as_slice()),
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..server::Config::default()
        });
        let options = Arc::new(options);
        let task = {
            let (accepts, links) = (Arc::clone(&accepts), Arc::clone(&links));
            let attempts = Arc::clone(&auth_attempts);
            let recorded = Arc::clone(&seen);
            let runs = Arc::clone(&exec);
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    accepts.fetch_add(1, Ordering::SeqCst);
                    let frozen = Arc::new(AtomicBool::new(false));
                    links
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(Arc::clone(&frozen));
                    let link = Link { stream, frozen };
                    let handler = Handler {
                        options: Arc::clone(&options),
                        attempts: Arc::clone(&attempts),
                        seen: Arc::clone(&recorded),
                        channels: HashMap::new(),
                        sftp: HashSet::new(),
                        exec: Arc::clone(&runs),
                    };
                    let config = Arc::clone(&config);
                    tokio::spawn(async move {
                        if let Ok(running) = server::run_stream(config, link, handler).await {
                            let _ = running.await;
                        }
                    });
                }
            })
        };
        Self {
            port,
            accepts,
            auth_attempts,
            seen,
            links,
            exec,
            task,
        }
    }

    /// What the exec channels did: runs stopped by a close, signals received.
    #[must_use]
    pub fn exec_runs(&self) -> &ExecRuns {
        &self.exec
    }

    /// The TCP connections accepted so far.
    #[must_use]
    pub fn accepts(&self) -> usize {
        self.accepts.load(Ordering::SeqCst)
    }

    /// The authentication requests received so far (password, public key, keyboard-interactive
    /// prompt and answer): what a client sent before it was let in.
    #[must_use]
    pub fn auth_attempts(&self) -> usize {
        self.auth_attempts.load(Ordering::SeqCst)
    }

    /// The forwarding requests clients sent: `agent`, `x11`, `tcpip_forward`,
    /// `streamlocal_forward`.
    #[must_use]
    pub fn forward_requests(&self) -> Vec<&'static str> {
        lock(&self.seen.requests).clone()
    }

    /// The channels the server tried to open towards a client, each with whether it was
    /// accepted.
    #[must_use]
    pub fn probes(&self) -> Vec<(&'static str, bool)> {
        lock(&self.seen.probes).clone()
    }

    /// Every connection accepted so far stops answering and swallows what it is sent.
    pub fn freeze(&self) {
        for link in self
            .links
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            link.store(true, Ordering::SeqCst);
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A port where a listener accepts TCP connections and never says a word: a hanging server.
///
/// # Panics
///
/// No port can be bound.
pub async fn silent_server() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let task = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    (port, task)
}

/// What clients asked for and what they took.
#[derive(Default)]
struct Seen {
    requests: Mutex<Vec<&'static str>>,
    probes: Mutex<Vec<(&'static str, bool)>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One connection's callbacks.
struct Handler {
    options: Arc<ServerOptions>,
    attempts: Arc<AtomicUsize>,
    seen: Arc<Seen>,
    /// Session channels no request has claimed yet; a subsystem takes its own.
    channels: HashMap<ChannelId, Channel<Msg>>,
    /// The channels SFTP runs on: their data is SFTP's, not echoed.
    sftp: HashSet<ChannelId>,
    exec: Arc<ExecRuns>,
}

impl Handler {
    fn requested(&self, what: &'static str) -> Result<bool, russh::Error> {
        lock(&self.seen.requests).push(what);
        Ok(false)
    }

    fn attempt(&self) {
        self.attempts.fetch_add(1, Ordering::SeqCst);
    }

    fn authorized(&self, key: &PublicKey) -> Auth {
        self.attempt();
        if self
            .options
            .authorized
            .iter()
            .any(|known| known.key_data() == key.key_data())
        {
            Auth::Accept
        } else {
            Auth::reject()
        }
    }
}

impl server::Handler for Handler {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.attempt();
        // russh hands a password over whatever the server offers: one that does not offer the
        // method refuses it, as a real server does.
        let offered = self.options.methods.contains(&MethodKind::Password);
        Ok(
            if offered && self.options.password.as_deref() == Some(password) {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(self.authorized(key))
    }

    async fn auth_publickey(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(self.authorized(key))
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        response: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        self.attempt();
        let Some(mut response) = response else {
            return Ok(Auth::Partial {
                name: Cow::Borrowed(""),
                instructions: Cow::Borrowed(""),
                prompts: Cow::Borrowed(PROMPTS),
            });
        };
        let expected = self.options.password.as_deref().map(str::as_bytes);
        Ok(match (response.next(), expected) {
            (Some(answer), Some(expected)) if &answer[..] == expected => Auth::Accept,
            _ => Auth::reject(),
        })
    }

    async fn auth_succeeded(&mut self, session: &mut Session) -> Result<(), Self::Error> {
        if self.options.probe_forwarding {
            let handle = session.handle();
            let seen = Arc::clone(&self.seen);
            tokio::spawn(async move {
                let agent = handle.channel_open_agent().await.is_ok();
                let x11 = handle.channel_open_x11("127.0.0.1", 6010).await.is_ok();
                let tcpip = handle
                    .channel_open_forwarded_tcpip("127.0.0.1", 8080, "127.0.0.1", 40_000)
                    .await
                    .is_ok();
                let shell = handle.channel_open_session().await.is_ok();
                lock(&seen.probes).extend([
                    ("agent", agent),
                    ("x11", x11),
                    ("forwarded_tcpip", tcpip),
                    ("session", shell),
                ]);
            });
        }
        Ok(())
    }

    async fn agent_request(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.requested("agent")
    }

    async fn x11_request(
        &mut self,
        _channel: ChannelId,
        _single_connection: bool,
        _protocol: &str,
        _cookie: &str,
        _screen: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.requested("x11").map(|_| ())
    }

    async fn tcpip_forward(
        &mut self,
        _address: &str,
        _port: &mut u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.requested("tcpip_forward")
    }

    async fn streamlocal_forward(
        &mut self,
        _socket_path: &str,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.requested("streamlocal_forward")
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Kept for a subsystem; any other request drops it, and the callbacks below see the rest.
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        self.exec.start(channel, data, session)
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.exec.eof(channel, session)
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.exec.closed(channel);
        Ok(())
    }

    async fn signal(
        &mut self,
        _channel: ChannelId,
        signal: Sig,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.exec.signal(&signal);
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _cols: u32,
        _rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        session.channel_success(channel)
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match (name, &self.options.sftp, self.channels.remove(&channel)) {
            ("sftp", Some(serve), Some(opened)) => {
                self.sftp.insert(channel);
                session.channel_success(channel)?;
                super::sftp::serve(opened, serve.clone());
                Ok(())
            }
            _ => session.channel_failure(channel),
        }
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.sftp.contains(&channel) || self.exec.swallows(channel) {
            return Ok(());
        }
        session.data(channel, data.to_vec())
    }
}
