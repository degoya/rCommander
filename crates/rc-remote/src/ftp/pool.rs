//! Connections to one server, reused and bounded (RC-070-03).
//!
//! An FTP control connection does one thing at a time, and a transfer occupies it until it is
//! finished. Every `Vfs` call therefore leases a connection of its own: an idle one when there
//! is one, a new login otherwise. A lease goes back to the pool when it is dropped, unless the
//! connection broke or timed out on the way, or the server still owes it a reply - a call
//! dropped half-way (a cancelled listing) - then it is closed.
//!
//! What a lease is for ([`Use`]) bounds how many there are: listings and the other commands
//! share one connection, so the panel stays usable while files move; transfers run on up to
//! [`FtpConfig::transfers`] more, and one past the limit waits until one ends. An idle
//! connection the server closed (a restart, a firewall's idle cut) is seen on its socket and
//! replaced before it is used; one idle for longer than [`STALE_AFTER`] is also checked with
//! `NOOP`, because servers close idle sessions (`421`) without telling; while it waits, a `NOOP` every
//! [`KEEPALIVE`] keeps it open, and after [`IDLE_TIMEOUT`] unused it is closed.
//!
//! Every connection holds a seat ([`Pool::seats`]): one for the commands and one per transfer
//! at most, fewer once the server refused a session as one too many (`seats.rs`).
//!
//! [`Pool::close`] ends the session on purpose (a disconnect): the idle connections say `QUIT`
//! and go, and none is kept from then on - a lease still out finishes its call and closes.

use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use suppaftp::FtpResult;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::{Notify, OwnedSemaphorePermit, Semaphore},
    time::MissedTickBehavior,
};

use super::{
    config::{Encryption, FtpConfig},
    connection::{Connection, ServerInfo, Transfer},
    encoding::Wire,
    error::{Failure, FtpError},
    tls::TlsSetup,
};

/// An idle connection older than this is checked before it is used again.
const STALE_AFTER: Duration = Duration::from_secs(15);
/// How often the idle connections are kept open with `NOOP`; below the 60 s many servers allow.
const KEEPALIVE: Duration = Duration::from_secs(30);
/// An idle connection unused for this long is closed; the next call logs in again.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// How long a closing session waits for its connections' answers to `QUIT`, all together.
const QUIT_WAIT: Duration = Duration::from_secs(2);

/// What a lease is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Use {
    /// A listing or another command: one connection serves them all, one after the other.
    Command,
    /// A download or an upload, held until the stream ends.
    Transfer,
}

struct Idle {
    connection: Connection,
    /// Its seat among the server's connections, given up when it closes.
    seat: OwnedSemaphorePermit,
    /// When it last served a call: [`IDLE_TIMEOUT`] counts from here.
    used: Instant,
    /// When it last answered: [`STALE_AFTER`] counts from here.
    checked: Instant,
}

/// The connections to one server, and what is known about it.
pub(crate) struct Pool {
    pub config: FtpConfig,
    pub info: ServerInfo,
    pub wire: Wire,
    /// The directory the first login started in (`PWD`); `/` when the server did not say.
    pub home: PathBuf,
    /// Whether passive data connections try `EPSV` before `PASV`: where `FEAT` offers it or the
    /// connection runs over IPv6, until the server refuses it and `PASV` works.
    epsv: AtomicBool,
    /// Whether `LIST` asks for dot files (`-a`, on a Unix server), until the server refuses the
    /// option.
    pub list_all: AtomicBool,
    tls: Option<TlsSetup>,
    idle: Mutex<Vec<Idle>>,
    /// Set by [`Pool::close`]: no connection is kept from then on.
    closed: AtomicBool,
    commands: Arc<Semaphore>,
    transfers: Arc<Semaphore>,
    /// One permit per connection the server may have from this pool; one is forgotten for each
    /// session the server refused as one too many.
    seats: Arc<Semaphore>,
    /// How many seats there are now.
    seat_count: AtomicUsize,
    /// The most sessions the server held for this pool at once (`seats.rs`).
    proven: AtomicUsize,
    /// A connection went back to the idle ones: a call waiting for a seat may take it.
    returned: Notify,
}

impl Pool {
    /// Logs in once - which proves the configuration - and keeps that connection; from then on
    /// a task keeps the idle connections open until the pool goes.
    pub async fn open(config: FtpConfig) -> Result<Arc<Self>, FtpError> {
        let tls = match config.encryption {
            Encryption::None => None,
            Encryption::Explicit | Encryption::Implicit => Some(TlsSetup::new(
                &config.host,
                config.port,
                Arc::clone(&config.trust),
            )?),
        };
        let seat_count = 1 + config.transfers.max(1);
        let seats = Arc::new(Semaphore::new(seat_count));
        let seat = Arc::clone(&seats)
            .try_acquire_owned()
            .expect("a new semaphore has its permits");
        let (mut connection, info) = Connection::open(&config, tls.as_ref(), None).await?;
        let pwd = tokio::time::timeout(config.timeout, connection.pwd()).await;
        // A `PWD` that timed out or failed may have left its reply unread: not kept (RC-080-40).
        let answered = matches!(pwd, Ok(Ok(_)));
        let home = match pwd {
            Ok(Ok(reply)) => home_of(&reply),
            _ => PathBuf::from("/"),
        };
        let epsv = info.epsv || connection.ipv6().await;
        let transfers = config.transfers.max(1);
        let pool = Arc::new(Self {
            wire: Wire::negotiate(info.utf8, config.encoding),
            config,
            info,
            home,
            epsv: AtomicBool::new(epsv),
            list_all: AtomicBool::new(info.unix),
            tls,
            idle: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
            commands: Arc::new(Semaphore::new(1)),
            transfers: Arc::new(Semaphore::new(transfers)),
            seats,
            seat_count: AtomicUsize::new(seat_count),
            proven: AtomicUsize::new(1),
            returned: Notify::new(),
        });
        if answered {
            pool.give_back(connection, seat);
        }
        tokio::spawn(keep_alive(Arc::downgrade(&pool)));
        Ok(pool)
    }

    /// A connection for one call of `purpose`, once one of its kind is free.
    pub async fn lease(self: &Arc<Self>, purpose: Use) -> Result<Lease, FtpError> {
        let gate = match purpose {
            Use::Command => &self.commands,
            Use::Transfer => &self.transfers,
        };
        let permit = Arc::clone(gate)
            .acquire_owned()
            .await
            .expect("the pool never closes its semaphores");
        let (connection, seat) = self.seated().await?;
        Ok(self.wrap(connection, seat, permit))
    }

    fn wrap(
        self: &Arc<Self>,
        connection: Connection,
        seat: OwnedSemaphorePermit,
        permit: OwnedSemaphorePermit,
    ) -> Lease {
        Lease {
            connection: Some(connection),
            seat: Some(seat),
            pool: Arc::clone(self),
            broken: false,
            pending: false,
            _permit: permit,
        }
    }

    fn idle(&self) -> std::sync::MutexGuard<'_, Vec<Idle>> {
        self.idle.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn give_back(&self, connection: Connection, seat: OwnedSemaphorePermit) {
        let now = Instant::now();
        self.keep(Idle {
            connection,
            seat,
            used: now,
            checked: now,
        });
    }

    /// Keeps `idle` for reuse, unless as many wait as can ever be in use at once or the session
    /// was closed: then it is closed.
    fn keep(&self, idle: Idle) {
        let mut waiting = self.idle();
        if !self.is_closed() && waiting.len() < 1 + self.config.transfers.max(1) {
            waiting.push(idle);
            drop(waiting);
            self.returned.notify_waiters();
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Ends the session: every idle connection says `QUIT` (waiting at most [`QUIT_WAIT`] for
    /// the answers) and is closed, the keepalive stops, and no connection is kept from now on. A
    /// lease still out - a running transfer - finishes its call; its connection is closed when
    /// it comes back, and a call after this logs in for itself alone.
    pub async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let waiting = std::mem::take(&mut *self.idle());
        let quit_all = async move {
            for Idle { mut connection, .. } in waiting {
                let _ = connection.quit().await;
            }
        };
        // Whatever has not answered by then is closed without its goodbye.
        let _ = tokio::time::timeout(QUIT_WAIT, quit_all).await;
    }

    /// One round of the keepalive: an idle connection unused for [`IDLE_TIMEOUT`] is closed,
    /// every other one gets a `NOOP` and stays if it answers.
    async fn refresh(&self) {
        let waiting = std::mem::take(&mut *self.idle());
        for Idle {
            mut connection,
            seat,
            used,
            ..
        } in waiting
        {
            if used.elapsed() >= IDLE_TIMEOUT {
                continue;
            }
            let alive = tokio::time::timeout(self.config.timeout, connection.noop()).await;
            if matches!(alive, Ok(Ok(()))) {
                self.keep(Idle {
                    connection,
                    seat,
                    used,
                    checked: Instant::now(),
                });
            }
        }
    }
}

/// The keepalive of one pool, until the pool is gone.
async fn keep_alive(pool: Weak<Pool>) {
    let mut ticks = tokio::time::interval(KEEPALIVE);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // The first tick is at once; the connection was just used.
    ticks.tick().await;
    loop {
        ticks.tick().await;
        let Some(pool) = pool.upgrade() else {
            return;
        };
        if pool.is_closed() {
            return;
        }
        pool.refresh().await;
    }
}

/// The directory a `PWD` reply names, by the rules SFTP takes a home by (`sftp/path.rs`): an
/// absolute path without `.`, `..`, `\` or a control character; `/` otherwise. The server is
/// not trusted to name one: a relative or odd answer is not followed.
fn home_of(reply: &str) -> PathBuf {
    crate::wire::home(Some(reply))
}

/// One connection, held for one call. Every command is bounded by the configured timeout.
pub(crate) struct Lease {
    connection: Option<Connection>,
    seat: Option<OwnedSemaphorePermit>,
    pool: Arc<Pool>,
    broken: bool,
    /// The server owes a reply not read yet: set before every command's `await` and while a
    /// transfer is open, taken back by [`Lease::settle`] once the reply is in. A lease dropped
    /// with it set - its call's future dropped half-way - closes the connection, which would
    /// hand the old reply to the next command otherwise (RC-080-40).
    pending: bool,
    /// Its place among the leases of its [`Use`]; released after the connection went back.
    _permit: OwnedSemaphorePermit,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let (Some(connection), Some(seat)) = (self.connection.take(), self.seat.take())
            && !self.broken
            && !self.pending
        {
            self.pool.give_back(connection, seat);
        }
    }
}

/// Runs one suppaftp call on the lease's connection, bounded and settled; a refusal is the
/// command `$name`'s.
macro_rules! command {
    ($lease:ident, $name:expr, $connection:ident => $call:expr) => {{
        let timeout = $lease.pool.config.timeout;
        $lease.pending = true;
        let $connection = $lease.connection();
        let result = tokio::time::timeout(timeout, $call).await;
        $lease.settle($name, result)
    }};
}

mod commands;
mod data;
mod listing;
mod seats;

pub(crate) use listing::Listing;

impl Lease {
    fn connection(&mut self) -> &mut Connection {
        self.connection
            .as_mut()
            .expect("a lease holds its connection until it is dropped")
    }

    /// Closes the connection instead of returning it, e.g. after an abandoned transfer.
    pub fn discard(&mut self) {
        self.broken = true;
    }

    /// The outcome of one call of `command`, whose reply is in: a refusal keeps the connection,
    /// anything else discards it.
    pub fn settle<T>(
        &mut self,
        command: &'static str,
        result: Result<FtpResult<T>, tokio::time::error::Elapsed>,
    ) -> Result<T, Failure> {
        let outcome = self.outcome(command, result);
        if !matches!(outcome, Err(Failure::Broken(_))) {
            self.pending = false;
        }
        outcome
    }

    /// What one step of `command` came to, the reply still owed or not.
    fn outcome<T>(
        &mut self,
        command: &'static str,
        result: Result<FtpResult<T>, tokio::time::error::Elapsed>,
    ) -> Result<T, Failure> {
        let failure = match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => Failure::of(command, error),
            Err(_) => Failure::Broken(io::Error::new(
                io::ErrorKind::TimedOut,
                "the FTP server did not answer in time",
            )),
        };
        if matches!(failure, Failure::Broken(_)) {
            self.broken = true;
        }
        Err(failure)
    }

    /// The timeout every step is bounded by.
    pub fn timeout(&self) -> Duration {
        self.pool.config.timeout
    }

    pub async fn retr(&mut self, path: &str) -> Result<Transfer, Failure> {
        self.open_data("RETR", &format!("RETR {path}")).await
    }

    pub async fn stor(&mut self, path: &str) -> Result<Transfer, Failure> {
        self.open_data("STOR", &format!("STOR {path}")).await
    }

    pub async fn appe(&mut self, path: &str) -> Result<Transfer, Failure> {
        self.open_data("APPE", &format!("APPE {path}")).await
    }

    /// Runs a listing command (`MLSD`, `LIST`, `LIST -a`) and returns its raw bytes, at most
    /// [`LISTING_MAX`].
    pub async fn listing(&mut self, command: &'static str) -> Result<Vec<u8>, Failure> {
        let name = command.split(' ').next().unwrap_or(command);
        let mut transfer = self.open_data(name, command).await?;
        let read =
            tokio::time::timeout(self.timeout(), read_capped(&mut transfer, LISTING_MAX)).await;
        let read = read.map(|result| result.map_err(suppaftp::FtpError::ConnectionError));
        // The data is not the reply: `226` is still owed.
        let bytes = match self.outcome(name, read) {
            Ok(bytes) => bytes,
            Err(failure) => {
                // The transfer is dropped unfinished; the connection is not reused.
                self.discard();
                return Err(failure);
            }
        };
        self.finish(transfer).await?;
        Ok(bytes)
    }

    /// Closes a transfer and reads its completion reply.
    pub async fn finish(&mut self, transfer: Transfer) -> Result<(), Failure> {
        let (timeout, command) = (self.timeout(), transfer.command);
        self.pending = true;
        let result = tokio::time::timeout(timeout, transfer.finish(self.connection())).await;
        self.settle(command, result)
    }
}

/// The most bytes a directory listing may have, some 300 000 lines: a server that sends more
/// is broken or hostile, and the listing is not held in memory.
pub(crate) const LISTING_MAX: u64 = 64 * 1024 * 1024;

/// All of `reader`, unless it holds more than `limit` bytes: then an `ftp_protocol` failure.
pub(crate) async fn read_capped<R: AsyncRead + Unpin>(
    reader: R,
    limit: u64,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(too_large(limit));
    }
    Ok(bytes)
}

/// The failure of a listing past `limit` bytes: an `ftp_protocol` one.
fn too_large(limit: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        FtpError::Protocol(format!(
            "the directory listing is larger than {limit} bytes"
        )),
    )
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
