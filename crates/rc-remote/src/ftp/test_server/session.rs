//! One control connection of the test server.

use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::TlsAcceptor;

use super::{Io, Options, PASSWORD, ServerTls, listing_line, stamp};

/// The acceptor of one control connection. With `require_reuse` it keeps the TLS sessions of
/// this control connection only, so a data connection resumes nothing but its own control
/// connection's session - vsftpd's strict reading of `require_ssl_reuse`.
fn session_acceptor(shared: &Shared) -> TlsAcceptor {
    if !shared.options.require_reuse {
        return shared.acceptor.clone();
    }
    let mut config = rustls::ServerConfig::clone(shared.acceptor.config());
    config.session_storage = rustls::server::ServerSessionMemoryCache::new(32);
    TlsAcceptor::from(Arc::new(config))
}

/// What every session of one server shares.
pub(super) struct Shared {
    /// Whether the temporary directory's file system is case-sensitive.
    pub case_sensitive: bool,
    pub root: PathBuf,
    pub options: Options,
    pub acceptor: TlsAcceptor,
    /// Counts every `PASS` command, before it is checked.
    pub passwords: Arc<std::sync::atomic::AtomicUsize>,
    /// The control connections open now, this one included.
    pub sessions: Arc<std::sync::atomic::AtomicUsize>,
    /// Ends every session (`TestServer::drop_sessions`).
    pub kick: tokio::sync::Notify,
}

/// How long the server waits for the client's data connection.
const DATA_TIMEOUT: Duration = Duration::from_secs(10);

struct Session {
    shared: Arc<Shared>,
    /// The TLS side of this control connection and its data connections.
    acceptor: TlsAcceptor,
    control: BufReader<Box<dyn Io>>,
    cwd: String,
    logged_in: bool,
    private_data: bool,
    rest: u64,
    pending: Option<TcpListener>,
    /// Where `PORT`/`EPRT` said to connect for the next data connection.
    active: Option<SocketAddr>,
    rename_from: Option<PathBuf>,
}

pub(super) async fn serve(shared: Arc<Shared>, socket: TcpStream) -> io::Result<()> {
    let acceptor = session_acceptor(&shared);
    let stream: Box<dyn Io> = if shared.options.tls == ServerTls::Implicit {
        Box::new(acceptor.accept(socket).await?)
    } else {
        Box::new(socket)
    };
    let cwd = shared.options.start.to_owned();
    let mut session = Session {
        shared,
        acceptor,
        control: BufReader::new(stream),
        cwd,
        logged_in: false,
        private_data: false,
        rest: 0,
        pending: None,
        active: None,
        rename_from: None,
    };
    let max = session.shared.options.max_sessions;
    if max > 0
        && session
            .shared
            .sessions
            .load(std::sync::atomic::Ordering::SeqCst)
            > max
    {
        return session
            .reply(&format!(
                "421 {max} users (the maximum) are already logged in, sorry"
            ))
            .await;
    }
    session.reply("220 rcommander test server").await?;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = tokio::select! {
            read = session.control.read_until(b'\n', &mut line) => read?,
            () = session.shared.kick.notified() => return Ok(()),
        };
        if read == 0 {
            return Ok(());
        }
        let text = session.decode(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        let (command, argument) = text.split_once(' ').unwrap_or((text, ""));
        let command = command.to_ascii_uppercase();
        if !session.run(&command, argument).await? {
            return Ok(());
        }
    }
}

impl Session {
    fn decode(&self, bytes: &[u8]) -> String {
        if self.shared.options.utf8 {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            bytes.iter().map(|&byte| char::from(byte)).collect()
        }
    }

    async fn reply(&mut self, text: &str) -> io::Result<()> {
        if let Some((held, wait)) = self.shared.options.hold
            && text.starts_with(held)
        {
            tokio::time::sleep(wait).await;
        }
        let stream = self.control.get_mut();
        stream.write_all(text.as_bytes()).await?;
        stream.write_all(b"\r\n").await?;
        stream.flush().await
    }

    /// The FTP path and the local path of an argument, `..` resolved inside the root.
    fn resolve(&self, argument: &str) -> (String, PathBuf) {
        let joined = if argument.starts_with('/') {
            argument.to_owned()
        } else {
            format!("{}/{argument}", self.cwd)
        };
        let mut parts: Vec<&str> = Vec::new();
        for part in joined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                name => parts.push(name),
            }
        }
        let local = parts
            .iter()
            .fold(self.shared.root.clone(), |path, part| path.join(part));
        (format!("/{}", parts.join("/")), local)
    }

    /// Runs one command; `false` ends the session.
    async fn run(&mut self, command: &str, argument: &str) -> io::Result<bool> {
        let open = matches!(
            command,
            "USER" | "PASS" | "AUTH" | "PBSZ" | "PROT" | "FEAT" | "SYST" | "QUIT" | "OPTS"
        );
        if !self.logged_in && !open {
            self.reply("530 Please login with USER and PASS.").await?;
            return Ok(true);
        }
        if command == "PASS" {
            self.shared
                .passwords
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let line = format!("{command} {argument}");
        if self
            .shared
            .options
            .refuse
            .iter()
            .any(|refused| line.trim_end() == *refused || line.starts_with(&format!("{refused} ")))
        {
            // A refused data command must not leave its listener for the next one.
            self.pending = None;
            self.reply("500 Command not understood.").await?;
            return Ok(true);
        }
        let (ftp, local) = self.resolve(argument);
        match command {
            "USER" => self.reply("331 Password required.").await?,
            "PASS" if argument == PASSWORD => {
                self.logged_in = true;
                self.reply("230 Login successful.").await?;
            }
            "PASS" => self.reply("530 Login incorrect.").await?,
            "AUTH" if self.shared.options.tls == ServerTls::Explicit => {
                self.reply("234 Proceed with negotiation.").await?;
                let plain = std::mem::replace(
                    &mut self.control,
                    // A stand-in while the TCP stream is upgraded.
                    BufReader::new(Box::new(tokio::io::duplex(1).0)),
                )
                .into_inner();
                let secure = self.acceptor.accept(plain).await?;
                self.control = BufReader::new(Box::new(secure));
            }
            "PBSZ" => self.reply("200 PBSZ=0").await?,
            "PROT" => {
                self.private_data = argument.eq_ignore_ascii_case("P");
                self.reply("200 Protection level set.").await?;
            }
            "FEAT" => self.reply(&self.features()).await?,
            // A case-insensitive disk is served as Windows, which the client reads as such.
            "SYST" if self.shared.case_sensitive => self.reply("215 UNIX Type: L8").await?,
            "SYST" => self.reply("215 Windows_NT").await?,
            "OPTS" if self.shared.options.utf8 => self.reply("200 Always in UTF8 mode.").await?,
            "TYPE" | "NOOP" | "MODE" | "STRU" => self.reply("200 OK.").await?,
            "PWD" => self.reply(&format!("257 \"{}\"", self.cwd)).await?,
            "CWD" if local.is_dir() => {
                self.cwd = ftp;
                self.reply("250 Directory successfully changed.").await?;
            }
            "SIZE" if local.is_file() => {
                let size = std::fs::metadata(&local)?.len();
                self.reply(&format!("213 {size}")).await?;
            }
            "MDTM" if local.is_file() => {
                let modified = std::fs::metadata(&local)?.modified()?;
                self.reply(&format!("213 {}", stamp(modified))).await?;
            }
            "MLST" if self.shared.options.mlst && local.symlink_metadata().is_ok() => {
                let mut text = format!("250-Listing {ftp}\r\n ").into_bytes();
                text.extend(listing_line(&local, &ftp, true, true));
                text.extend(b"\r\n250 End");
                let stream = self.control.get_mut();
                stream.write_all(&text).await?;
                stream.write_all(b"\r\n").await?;
                stream.flush().await?;
            }
            "MFMT" if self.shared.options.mlst => self.mfmt(argument).await?,
            "MKD" if !local.exists() && std::fs::create_dir(&local).is_ok() => {
                self.reply(&format!("257 \"{ftp}\" created")).await?;
            }
            "RMD" if std::fs::remove_dir(&local).is_ok() => self.reply("250 Removed.").await?,
            "DELE" if local.symlink_metadata().is_ok_and(|meta| !meta.is_dir()) => {
                std::fs::remove_file(&local)?;
                self.reply("250 Deleted.").await?;
            }
            "RNFR" if local.symlink_metadata().is_ok() => {
                self.rename_from = Some(local);
                self.reply("350 Ready for RNTO.").await?;
            }
            "RNTO" => match self.rename_from.take() {
                Some(from) if std::fs::rename(&from, &local).is_ok() => {
                    self.reply("250 Rename successful.").await?;
                }
                _ => self.reply("550 Rename failed.").await?,
            },
            "REST" => {
                self.rest = argument.trim().parse().unwrap_or(0);
                self.reply("350 Restart position accepted.").await?;
            }
            "PASV" | "EPSV" => self.passive(command).await?,
            "PORT" | "EPRT" => match port_address(command, argument) {
                Some(address) => {
                    self.pending = None;
                    self.active = Some(address);
                    self.reply("200 PORT command successful.").await?;
                }
                None => self.reply("501 Bad address.").await?,
            },
            "RETR" if local.is_file() => self.retrieve(&local).await?,
            "STOR" | "APPE" if !local.is_dir() && local.parent().is_some_and(Path::is_dir) => {
                self.store(&local, command == "APPE").await?;
            }
            "LIST" | "MLSD" => self.listing(command, argument).await?,
            "QUIT" => {
                self.reply("221 Goodbye.").await?;
                return Ok(false);
            }
            "MLST" if self.shared.options.mlst => self.reply("550 No such file.").await?,
            "AUTH" | "OPTS" | "MLST" | "MFMT" => self.reply("502 Not implemented.").await?,
            "CWD" | "SIZE" | "MDTM" | "MKD" | "RMD" | "DELE" | "RNFR" | "RETR" | "STOR"
            | "APPE" => {
                // A refused data command must not leave its listener for the next one.
                self.pending = None;
                self.reply("550 Failed.").await?;
            }
            _ => self.reply("502 Command not implemented.").await?,
        }
        Ok(true)
    }

    fn features(&self) -> String {
        let options = self.shared.options;
        let mut lines = vec!["211-Features:", " SIZE", " MDTM", " REST STREAM", " EPSV"];
        if options.mlst {
            lines.extend([" MLST type*;size*;modify*;UNIX.mode*;", " MFMT"]);
        }
        if options.utf8 {
            lines.push(" UTF8");
        }
        if options.tls == ServerTls::Explicit {
            lines.extend([" AUTH TLS", " PBSZ", " PROT"]);
        }
        lines.push("211 End");
        lines.join("\r\n")
    }

    async fn mfmt(&mut self, argument: &str) -> io::Result<()> {
        let parsed = argument.split_once(' ').and_then(|(time, path)| {
            Some((super::super::time::parse_stamp(time)?, self.resolve(path)))
        });
        match parsed {
            Some((time, (ftp, local))) if local.exists() => {
                filetime::set_file_mtime(&local, filetime::FileTime::from_system_time(time))?;
                self.reply(&format!("213 Modify={}; {ftp}", stamp(time)))
                    .await
            }
            _ => self.reply("550 Failed.").await,
        }
    }

    async fn passive(&mut self, command: &str) -> io::Result<()> {
        self.active = None;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        self.pending = Some(listener);
        if command == "EPSV" {
            self.reply(&format!("229 Entering Extended Passive Mode (|||{port}|)"))
                .await
        } else {
            let [a, b, c, d] = self.shared.options.pasv_address;
            self.reply(&format!(
                "227 Entering Passive Mode ({a},{b},{c},{d},{},{})",
                port / 256,
                port % 256
            ))
            .await
        }
    }

    /// Announces the transfer and accepts the data connection, TLS after `PROT P`.
    async fn data(&mut self) -> io::Result<Box<dyn Io>> {
        if let Some(address) = self.active.take() {
            self.reply("150 Opening data connection.").await?;
            let socket = tokio::time::timeout(DATA_TIMEOUT, TcpStream::connect(address))
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
            return self.data_stream(socket).await;
        }
        let Some(listener) = self.pending.take() else {
            self.reply("425 Use PASV first.").await?;
            return Err(io::Error::other("no data connection"));
        };
        self.reply("150 Opening data connection.").await?;
        let (socket, _) = tokio::time::timeout(DATA_TIMEOUT, listener.accept())
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
        self.data_stream(socket).await
    }

    /// The data connection on `socket`: TLS after `PROT P`. With `require_reuse` a handshake
    /// that resumes no session of this control connection is refused, as vsftpd's
    /// `require_ssl_reuse` does.
    async fn data_stream(&self, socket: TcpStream) -> io::Result<Box<dyn Io>> {
        if !self.private_data {
            return Ok(Box::new(socket));
        }
        let secure = self.acceptor.accept(socket).await?;
        let resumed = secure.get_ref().1.handshake_kind() == Some(rustls::HandshakeKind::Resumed);
        if self.shared.options.require_reuse && !resumed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the data connection did not resume a TLS session",
            ));
        }
        Ok(Box::new(secure))
    }

    async fn retrieve(&mut self, local: &Path) -> io::Result<()> {
        let content = std::fs::read(local)?;
        let offset = usize::try_from(std::mem::take(&mut self.rest))
            .unwrap_or(usize::MAX)
            .min(content.len());
        let mut data = self.data().await?;
        if self.shared.options.stall {
            // A few bytes, then nothing: the client's idle limit has to end it.
            data.write_all(
                &content[offset..]
                    .iter()
                    .take(4)
                    .copied()
                    .collect::<Vec<_>>(),
            )
            .await?;
            data.flush().await?;
            std::future::pending::<()>().await;
        }
        data.write_all(&content[offset..]).await?;
        data.shutdown().await?;
        drop(data);
        self.reply("226 Transfer complete.").await
    }

    async fn store(&mut self, local: &Path, append: bool) -> io::Result<()> {
        let mut data = self.data().await?;
        if self.shared.options.stall {
            // Four bytes, then nothing is read: the client's buffers fill and its idle limit
            // has to end the upload.
            data.read_exact(&mut [0; 4]).await?;
            std::future::pending::<()>().await;
        }
        let mut received = Vec::new();
        data.read_to_end(&mut received).await?;
        drop(data);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(local)?;
        std::io::Write::write_all(&mut file, &received)?;
        self.reply("226 Transfer complete.").await
    }

    async fn listing(&mut self, command: &str, argument: &str) -> io::Result<()> {
        let mlsd = command == "MLSD";
        if mlsd && !self.shared.options.mlst {
            self.pending = None;
            return self.reply("500 Unknown command.").await;
        }
        let show_all = argument.split_whitespace().any(|word| word == "-a");
        let path = argument
            .split_whitespace()
            .find(|word| !word.starts_with('-'))
            .unwrap_or("");
        let (_, dir) = self.resolve(path);
        let Ok(read) = std::fs::read_dir(&dir) else {
            self.pending = None;
            return self.reply("550 Failed to open directory.").await;
        };
        let mut names: Vec<String> = read
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| mlsd || show_all || !name.starts_with('.'))
            .collect();
        names.sort();
        if mlsd || show_all {
            names.insert(0, "..".to_owned());
            names.insert(0, ".".to_owned());
        }
        let utf8 = self.shared.options.utf8;
        let mut body = Vec::new();
        for name in &names {
            let entry = match name.as_str() {
                "." | ".." => dir.clone(),
                _ => dir.join(name),
            };
            body.extend(listing_line(&entry, name, mlsd, utf8));
            body.extend(b"\r\n");
        }
        let mut data = self.data().await?;
        data.write_all(&body).await?;
        data.shutdown().await?;
        drop(data);
        self.reply("226 Directory send OK.").await
    }
}

/// The address of `PORT h1,h2,h3,h4,p1,p2` or `EPRT |proto|address|port|`.
fn port_address(command: &str, argument: &str) -> Option<SocketAddr> {
    if command == "PORT" {
        let numbers: Vec<u8> = argument
            .trim()
            .split(',')
            .map(|part| part.parse().ok())
            .collect::<Option<_>>()?;
        let [a, b, c, d, high, low] = numbers.as_slice() else {
            return None;
        };
        let ip = std::net::Ipv4Addr::new(*a, *b, *c, *d);
        return Some(SocketAddr::new(
            ip.into(),
            (u16::from(*high) << 8) | u16::from(*low),
        ));
    }
    let delimiter = argument.trim().chars().next()?;
    let mut parts = argument.trim().split(delimiter).skip(2);
    let ip = parts.next()?.parse().ok()?;
    let port = parts.next()?.parse().ok()?;
    Some(SocketAddr::new(ip, port))
}
