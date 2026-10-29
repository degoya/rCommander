//! One logged-in control connection, plain or TLS, and its data transfers.
//!
//! suppaftp types plain FTP and FTPS as two stream types (`ImplAsyncFtpStream` is generic over
//! its TLS stream), so both are unified behind one enum here, after rDownloader's `rd-ftp`.
//! Opening a connection is bounded by the configured timeout as a whole: a server that accepts
//! the socket and then says nothing cannot hold a caller forever.
//!
//! suppaftp carries the control connection only. Data connections, passive and active, are
//! opened here, one step per command ([`Connection::passive_port`], [`Connection::open_passive`],
//! [`Connection::listen_active`], [`Connection::open_active`]), so the pool knows which command
//! a server refused - `EPSV` refused falls back to `PASV` there - and can name it.

use std::{
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

use secrecy::ExposeSecret;
use suppaftp::{
    FtpResult, Status,
    tokio::{AsyncFtpStream, AsyncRustlsConnector, AsyncRustlsFtpStream},
    types::{Features, FileType, Response},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{TlsConnector, client::TlsStream};

use super::{
    active::{self, close_data},
    config::{Encryption, FtpConfig},
    error::{FtpError, reply_text},
    passive,
    tls::{TlsSetup, connect_data},
};

/// What a server said about itself at the first login (`FEAT`, `SYST`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ServerInfo {
    /// `FEAT` lists `UTF8`: names go out and come back in UTF-8 after `OPTS UTF8 ON`.
    pub utf8: bool,
    /// `MLST`/`MLSD` (RFC 3659): machine-readable listings and `stat`.
    pub mlst: bool,
    /// `MFMT`: modification times can be set.
    pub mfmt: bool,
    /// `EPSV` (RFC 2428), tried before `PASV`.
    pub epsv: bool,
    /// `SYST` says `UNIX`: `LIST -a` shows dot files, permissions are Unix bits.
    pub unix: bool,
    /// Not a Windows server (`SYST` names neither Windows nor FileZilla's emulation).
    pub case_sensitive: bool,
}

impl ServerInfo {
    fn from_replies(features: &Features, syst: &str) -> Self {
        let has = |name: &str| features.keys().any(|key| key.eq_ignore_ascii_case(name));
        let syst = syst.to_ascii_uppercase();
        Self {
            utf8: has("UTF8"),
            mlst: has("MLST") || has("MLSD"),
            mfmt: has("MFMT"),
            epsv: has("EPSV"),
            unix: syst.starts_with("UNIX"),
            case_sensitive: !(syst.contains("WINDOWS") || syst.contains("FILEZILLA")),
        }
    }
}

/// An established, logged-in control connection.
pub(crate) enum Connection {
    Plain(Box<AsyncFtpStream>),
    /// With its connector: verifier and resumption store, for its active data connections.
    Secure(Box<AsyncRustlsFtpStream>, TlsConnector),
}

/// Dispatches one method over both stream types; the arms differ only in the receiver's type.
macro_rules! dispatch {
    ($self:expr, $stream:ident => $call:expr) => {
        match $self {
            Connection::Plain($stream) => $call,
            Connection::Secure($stream, _) => $call,
        }
    };
}

/// Maps a failure before the login completed. A `421` at the greeting is the server's "too
/// many users" (pure-ftpd, vsftpd's `max_clients`/`max_per_ip`).
fn early(error: suppaftp::FtpError) -> FtpError {
    match error {
        suppaftp::FtpError::ConnectionError(source) => FtpError::Connect(source),
        suppaftp::FtpError::UnexpectedResponse(response) => {
            let text = reply_text(&response.body);
            if response.status.code() == 421 {
                FtpError::TooManySessions(text)
            } else {
                FtpError::Protocol(text)
            }
        }
        other => FtpError::Protocol(other.to_string()),
    }
}

/// One byte of `socket` looked at, not taken, without waiting.
fn peek(socket: &TcpStream) -> Poll<io::Result<usize>> {
    let mut byte = [0_u8; 1];
    let mut buffer = ReadBuf::new(&mut byte);
    socket.poll_peek(&mut Context::from_waker(Waker::noop()), &mut buffer)
}

/// A refused `USER`/`PASS`: the login itself, unless the server says it has too many sessions
/// (`421`, or ProFTPD's `530 Sorry, the maximum number of clients ...`).
fn refused_login(response: &Response) -> FtpError {
    let text = reply_text(&response.body);
    let lower = text.to_ascii_lowercase();
    let full = lower.contains("too many") || lower.contains("maximum number");
    match response.status.code() {
        421 => FtpError::TooManySessions(text),
        530 if full => FtpError::TooManySessions(text),
        _ => FtpError::LoginFailed,
    }
}

impl Connection {
    /// Connects, negotiates TLS, logs in, switches to binary and to UTF-8 where offered.
    /// `known` is the server's description from an earlier connection; without it `FEAT` and
    /// `SYST` are asked.
    pub async fn open(
        config: &FtpConfig,
        tls: Option<&TlsSetup>,
        known: Option<ServerInfo>,
    ) -> Result<(Self, ServerInfo), FtpError> {
        tokio::time::timeout(config.timeout, Self::handshake(config, tls, known))
            .await
            .unwrap_or(Err(FtpError::Timeout))
    }

    async fn handshake(
        config: &FtpConfig,
        tls: Option<&TlsSetup>,
        known: Option<ServerInfo>,
    ) -> Result<(Self, ServerInfo), FtpError> {
        let address = (config.host.as_str(), config.port);
        let mut connection = match (config.encryption, tls) {
            (Encryption::None, _) => Self::Plain(Box::new(
                AsyncFtpStream::connect(address).await.map_err(early)?,
            )),
            (Encryption::Explicit, Some(tls)) => {
                let session = tls.session();
                // Typed as the TLS stream from the start: `into_secure` upgrades in place.
                let plain = AsyncRustlsFtpStream::connect(address)
                    .await
                    .map_err(early)?;
                let secure = plain
                    .into_secure(AsyncRustlsConnector::from(session.clone()), &config.host)
                    .await
                    .map_err(|error| match error {
                        suppaftp::FtpError::SecureError(message) => tls.handshake_error(message),
                        suppaftp::FtpError::UnexpectedResponse(_) => FtpError::TlsRefused,
                        other => early(other),
                    })?;
                Self::Secure(Box::new(secure), session)
            }
            (Encryption::Implicit, Some(tls)) => {
                let session = tls.session();
                let mut secure = AsyncRustlsFtpStream::connect_secure_implicit(
                    address,
                    AsyncRustlsConnector::from(session.clone()),
                    &config.host,
                )
                .await
                .map_err(|error| match error {
                    suppaftp::FtpError::SecureError(message) => tls.handshake_error(message),
                    other => early(other),
                })?;
                // `connect_secure_implicit` leaves the data channel to the caller.
                for command in ["PBSZ 0", "PROT P"] {
                    secure
                        .custom_command(command, &[Status::CommandOk])
                        .await
                        .map_err(early)?;
                }
                Self::Secure(Box::new(secure), session)
            }
            (Encryption::Explicit | Encryption::Implicit, None) => {
                return Err(FtpError::Tls("no TLS configuration".to_owned()));
            }
        };

        let (user, password) = config.login();
        let password = password.expose_secret();
        let login = dispatch!(&mut connection, stream => stream.login(user, password).await);
        login.map_err(|error| match error {
            suppaftp::FtpError::UnexpectedResponse(response) => refused_login(&response),
            other => early(other),
        })?;
        // Everything is transferred as bytes; ASCII mode would rewrite line endings.
        let binary =
            dispatch!(&mut connection, stream => stream.transfer_type(FileType::Binary).await);
        binary.map_err(early)?;

        let info = match known {
            Some(info) => info,
            None => connection.describe().await?,
        };
        if info.utf8 {
            let reply = connection
                .custom(
                    "OPTS UTF8 ON",
                    &[Status::CommandOk, Status::CommandNotImplemented],
                )
                .await;
            if let Err(suppaftp::FtpError::ConnectionError(source)) = reply {
                return Err(FtpError::Connect(source));
            }
        }
        Ok((connection, info))
    }

    /// `FEAT` and `SYST`; a server without either is described by what is left.
    async fn describe(&mut self) -> Result<ServerInfo, FtpError> {
        let features = match dispatch!(self, stream => stream.feat().await) {
            Ok(features) => features,
            Err(suppaftp::FtpError::ConnectionError(source)) => {
                return Err(FtpError::Connect(source));
            }
            Err(_) => Features::new(),
        };
        let syst = match self.custom("SYST", &[Status::Name]).await {
            Ok(response) => reply_text(&response.body),
            Err(suppaftp::FtpError::ConnectionError(source)) => {
                return Err(FtpError::Connect(source));
            }
            Err(_) => String::new(),
        };
        Ok(ServerInfo::from_replies(&features, &syst))
    }

    /// The server's address on the control connection: where every data connection goes.
    async fn peer(&mut self) -> FtpResult<SocketAddr> {
        dispatch!(self, stream => stream.get_ref().await.peer_addr())
            .map_err(suppaftp::FtpError::ConnectionError)
    }

    /// Whether the control connection runs over IPv6, where `PASV` cannot name an address.
    pub async fn ipv6(&mut self) -> bool {
        self.peer().await.is_ok_and(|peer| peer.is_ipv6())
    }

    pub async fn custom(&mut self, command: &str, expected: &[Status]) -> FtpResult<Response> {
        dispatch!(self, stream => stream.custom_command(command, expected).await)
    }

    pub async fn noop(&mut self) -> FtpResult<()> {
        dispatch!(self, stream => stream.noop().await)
    }

    /// Whether the server closed this idle connection, seen without a round trip: `Some(true)`
    /// when the socket is at its end or failed, `Some(false)` when nothing waits on it, `None`
    /// when bytes wait (a `421` before a close, a TLS record) and a `NOOP` has to decide.
    pub async fn closed(&mut self) -> Option<bool> {
        let peeked = dispatch!(self, stream => peek(&*stream.get_ref().await));
        match peeked {
            Poll::Pending => Some(false),
            Poll::Ready(Ok(0) | Err(_)) => Some(true),
            Poll::Ready(Ok(_)) => None,
        }
    }

    /// `QUIT`: the server ends the session; the connection is closed when dropped.
    pub async fn quit(&mut self) -> FtpResult<()> {
        dispatch!(self, stream => stream.quit().await)
    }

    pub async fn pwd(&mut self) -> FtpResult<String> {
        dispatch!(self, stream => stream.pwd().await)
    }

    pub async fn cwd(&mut self, path: &str) -> FtpResult<()> {
        dispatch!(self, stream => stream.cwd(path).await)
    }

    pub async fn mkdir(&mut self, path: &str) -> FtpResult<()> {
        dispatch!(self, stream => stream.mkdir(path).await)
    }

    pub async fn rmdir(&mut self, path: &str) -> FtpResult<()> {
        dispatch!(self, stream => stream.rmdir(path).await)
    }

    pub async fn remove(&mut self, path: &str) -> FtpResult<()> {
        dispatch!(self, stream => stream.rm(path).await)
    }

    pub async fn rename(&mut self, from: &str, to: &str) -> FtpResult<()> {
        dispatch!(self, stream => stream.rename(from, to).await)
    }

    pub async fn size(&mut self, path: &str) -> FtpResult<usize> {
        dispatch!(self, stream => stream.size(path).await)
    }

    pub async fn mlst(&mut self, path: &str) -> FtpResult<String> {
        dispatch!(self, stream => stream.mlst(Some(path)).await)
    }

    pub async fn site(&mut self, command: &str) -> FtpResult<Response> {
        dispatch!(self, stream => stream.site(command).await)
    }

    /// `REST <offset>`: the next `RETR` starts there.
    pub async fn rest(&mut self, offset: u64) -> FtpResult<()> {
        let offset = usize::try_from(offset).map_err(|_| {
            suppaftp::FtpError::ConnectionError(io::Error::new(
                io::ErrorKind::InvalidInput,
                "offset beyond this platform's address space",
            ))
        })?;
        dispatch!(self, stream => stream.resume_transfer(offset).await)
    }

    /// The port of a passive data connection: `EPSV` (`229 (|||port|)`) when `extended`, else
    /// `PASV` (`227 (h1,h2,h3,h4,p1,p2)`). The address a `PASV` reply names is not followed: the
    /// data connection always goes to the control connection's peer, so a server cannot point a
    /// transfer elsewhere, and one behind NAT that names its private address still works.
    pub async fn passive_port(&mut self, extended: bool) -> FtpResult<u16> {
        let (command, status) = if extended {
            ("EPSV", Status::ExtendedPassiveMode)
        } else {
            ("PASV", Status::PassiveMode)
        };
        let reply = self.custom(command, &[status]).await?;
        let text = String::from_utf8_lossy(&reply.body);
        let port = if extended {
            passive::epsv_port(&text)
        } else {
            passive::pasv_port(&text)
        };
        port.ok_or(suppaftp::FtpError::UnexpectedResponse(reply))
    }

    /// A passive data connection for `command` (`RETR <path>`, `MLSD`, ...), named `name`:
    /// connected to `port` on the server first, then the command, then TLS over FTPS (`PROT P`)
    /// to `host`.
    pub async fn open_passive(
        &mut self,
        port: u16,
        (name, command): (&'static str, &str),
        host: &str,
        timeout: Duration,
    ) -> FtpResult<Transfer> {
        let broken = suppaftp::FtpError::ConnectionError;
        let mut server = self.peer().await?;
        server.set_port(port);
        let socket = tokio::time::timeout(timeout, TcpStream::connect(server))
            .await
            .map_err(|_| {
                broken(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the FTP server did not accept the data connection",
                ))
            })?
            .map_err(broken)?;
        self.custom(command, &[Status::AboutToSend, Status::AlreadyOpen])
            .await?;
        self.data_over(socket, host, name).await
    }

    /// A listener for an active data connection, on the control connection's local address
    /// (`active.rs`), and the `PORT`/`EPRT` command that names it to the server.
    pub async fn listen_active(&mut self) -> FtpResult<(TcpListener, String)> {
        let broken = suppaftp::FtpError::ConnectionError;
        let local =
            dispatch!(self, stream => stream.get_ref().await.local_addr()).map_err(broken)?;
        let listener = active::listen(local.ip()).await.map_err(broken)?;
        let address = listener.local_addr().map_err(broken)?;
        Ok((listener, active::port_command(address)))
    }

    /// The active data connection for `command`, named `name`, after `PORT`/`EPRT` named
    /// `listener`: the
    /// command, then exactly one connection from the server's control address (`active.rs`).
    /// Over FTPS the data connection is TLS (`PROT P`) to `host`.
    pub async fn open_active(
        &mut self,
        listener: TcpListener,
        (name, command): (&'static str, &str),
        host: &str,
        timeout: Duration,
    ) -> FtpResult<Transfer> {
        let server = self.peer().await?;
        self.custom(command, &[Status::AboutToSend, Status::AlreadyOpen])
            .await?;
        let socket = active::accept(&listener, server.ip(), timeout)
            .await
            .map_err(suppaftp::FtpError::ConnectionError)?;
        drop(listener);
        self.data_over(socket, host, name).await
    }

    /// The data connection on `socket`: TLS over FTPS, with this control connection's
    /// connector - its verifier and its resumption store.
    async fn data_over(
        &self,
        socket: TcpStream,
        host: &str,
        command: &'static str,
    ) -> FtpResult<Transfer> {
        let data = match self {
            Self::Secure(_, session) => Data::Secure(Box::new(
                connect_data(session, host, socket)
                    .await
                    .map_err(|error| suppaftp::FtpError::SecureError(error.to_string()))?,
            )),
            Self::Plain(_) => Data::Plain(socket),
        };
        Ok(Transfer { command, data })
    }

    /// The completion reply of a transfer (`226`, or `250`).
    async fn transfer_reply(&mut self) -> FtpResult<()> {
        let expected = [Status::ClosingDataConnection, Status::RequestedFileActionOk];
        dispatch!(self, stream => stream.read_response_in(&expected).await).map(|_| ())
    }
}

/// An open data connection: read a download, write an upload, then [`Transfer::finish`].
pub(crate) struct Transfer {
    /// The command it carries the data of (`RETR`, `STOR`, `APPE`, `MLSD`, `LIST`), whose
    /// completion reply ends it.
    pub command: &'static str,
    data: Data,
}

enum Data {
    Plain(TcpStream),
    Secure(Box<TlsStream<TcpStream>>),
}

impl Transfer {
    /// Closes the data connection and reads the server's completion reply on `connection`, the
    /// control connection that opened it. It is closed with `close_notify` (TLS) and drained
    /// first (`close_data`), so no unread session ticket turns the close into a reset.
    pub async fn finish(mut self, connection: &mut Connection) -> FtpResult<()> {
        let closed = close_data(&mut self).await;
        drop(self);
        let reply = connection.transfer_reply().await;
        closed.map_err(suppaftp::FtpError::ConnectionError)?;
        reply
    }
}

impl AsyncRead for Transfer {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_read(context, buffer),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Transfer {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_write(context, bytes),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_write(context, bytes),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_flush(context),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_shutdown(context),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_shutdown(context),
        }
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
