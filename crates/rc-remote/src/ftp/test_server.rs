//! An in-process FTP and FTPS server over a temporary directory, for the backend's tests.
//!
//! No FTP server can be assumed on the machine that runs the tests, and none of the pure-Rust
//! server crates is in the dependency tree, so this speaks the command subset `FtpVfs` uses
//! against the real file system: enough to run the `rc-vfs` conformance suite with `MLSD` and
//! with `LIST` only, over plain FTP, explicit and implicit FTPS (a self-signed certificate made
//! per run), and with Latin-1 names on the wire; passive (`PASV`, `EPSV`) or active (`PORT`,
//! `EPRT`) data connections; a reply can be held back. Every switch is an [`Options`] field.
//! This crate's tests use it, and the engine's with the feature `testkit` (RC-070-03).

mod session;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tokio_rustls::TlsAcceptor;

use super::time::Civil;

/// The password the server accepts for every user.
pub const PASSWORD: &str = "test-password";

/// How the server encrypts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerTls {
    None,
    Explicit,
    Implicit,
}

/// What the server offers.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub tls: ServerTls,
    /// `MLSD`, `MLST` and `MFMT` in `FEAT` and served; without, only `LIST`, `SIZE`, `MDTM`.
    pub mlst: bool,
    /// `UTF8` in `FEAT`; without it names go out in Latin-1.
    pub utf8: bool,
    /// TLS 1.3 session tickets after each handshake, as real servers send them.
    pub tickets: bool,
    /// `RETR` sends four bytes and then nothing, holding the data connection open; `STOR`
    /// reads four bytes and then nothing.
    pub stall: bool,
    /// A TLS data connection must resume a session of its own control connection (vsftpd's
    /// `require_ssl_reuse`); implies `tickets`, without which TLS 1.3 resumes nothing.
    pub require_reuse: bool,
    /// The directory every session starts in, what `PWD` answers before a `CWD`.
    pub start: &'static str,
    /// Commands answered `500` whatever `FEAT` says, as servers behind a firewall or with a
    /// feature switched off do: `EPSV`, or `LIST -a` to refuse that command with that argument.
    pub refuse: &'static [&'static str],
    /// The address a `PASV` reply names instead of `127.0.0.1`, as a server behind NAT or a
    /// hostile one does; the server still listens on `127.0.0.1` only.
    pub pasv_address: [u8; 4],
    /// Sessions at once the server takes; one more is greeted with `421 ... (the maximum)` and
    /// closed, as pure-ftpd does. 0 takes any number.
    pub max_sessions: usize,
    /// How long a closed session still counts towards `max_sessions`: pure-ftpd lets go of a
    /// session some milliseconds after the client closed it.
    pub linger: Duration,
    /// A reply that starts with the text (`250`, `226`, `150 Opening`) is sent only this long
    /// after it is due, as a slow server answers: what a client that gives up on a command
    /// half-way meets (RC-080-40).
    pub hold: Option<(&'static str, Duration)>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tls: ServerTls::None,
            mlst: true,
            utf8: true,
            tickets: false,
            stall: false,
            require_reuse: false,
            start: "/",
            refuse: &[],
            pasv_address: [127, 0, 0, 1],
            max_sessions: 0,
            linger: Duration::ZERO,
            hold: None,
        }
    }
}

/// Any stream a session runs over: TCP, or TLS on TCP.
pub(crate) trait Io: AsyncRead + AsyncWrite + Unpin + Send + Sync {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> Io for T {}

/// A running server; it stops with the test's runtime.
pub struct TestServer {
    pub port: u16,
    /// The server's certificate, DER: what a test trusts when it trusts this server.
    pub certificate: Vec<u8>,
    root: tempfile::TempDir,
    /// How many `PASS` commands the server received, any session.
    passwords: Arc<AtomicUsize>,
    /// How many control connections are open now.
    sessions: Arc<AtomicUsize>,
    shared: Arc<session::Shared>,
}

impl TestServer {
    pub async fn start(options: Options) -> Self {
        let root = tempfile::tempdir().expect("server root");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let (certificate, acceptor) = tls_acceptor();
        let acceptor = if options.tickets || options.require_reuse {
            let mut config = rustls::ServerConfig::clone(acceptor.config());
            config.send_tls13_tickets = 2;
            TlsAcceptor::from(Arc::new(config))
        } else {
            acceptor
        };
        let passwords = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let shared = Arc::new(session::Shared {
            sessions: Arc::clone(&sessions),
            kick: tokio::sync::Notify::new(),
            passwords: Arc::clone(&passwords),
            case_sensitive: case_sensitive(root.path()),
            root: root.path().to_path_buf(),
            options,
            acceptor,
        });
        let open = Arc::clone(&sessions);
        let server = Arc::clone(&shared);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let shared = Arc::clone(&shared);
                let open = Arc::clone(&open);
                open.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let linger = shared.options.linger;
                    let _ = session::serve(shared, socket).await;
                    tokio::time::sleep(linger).await;
                    open.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        Self {
            port,
            certificate,
            root,
            passwords,
            sessions,
            shared: server,
        }
    }

    /// Closes every session at once, as a server restart or a firewall's idle cut does; the
    /// server keeps listening.
    pub fn drop_sessions(&self) {
        self.shared.kick.notify_waiters();
    }

    /// How many `PASS` commands reached the server so far.
    pub fn passwords_received(&self) -> usize {
        self.passwords.load(Ordering::SeqCst)
    }

    /// How many control connections are open now: a session the client closed (`QUIT`, or the
    /// connection dropped) no longer counts.
    pub fn open_sessions(&self) -> usize {
        self.sessions.load(Ordering::SeqCst)
    }

    /// The local file behind the FTP path `/relative`.
    pub fn local(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative.trim_start_matches('/'))
    }
}

/// Whether the file system under `dir` tells names apart by case (not NTFS, not APFS by
/// default): what the server's `SYST` reports, so the client's capability matches the disk.
fn case_sensitive(dir: &Path) -> bool {
    let probe = dir.join("case-probe");
    std::fs::write(&probe, b"").expect("probe file");
    let sensitive = !dir.join("CASE-PROBE").exists();
    std::fs::remove_file(&probe).expect("remove the probe file");
    sensitive
}

/// A self-signed certificate for `127.0.0.1` and the acceptor that presents it.
fn tls_acceptor() -> (Vec<u8>, TlsAcceptor) {
    let generated =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).expect("certificate");
    let certificate: CertificateDer<'static> = generated.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        generated.signing_key.serialize_der(),
    ));
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], key)
        .expect("server certificate");
    // No TLS 1.3 session tickets: an upload never reads its data connection, so a ticket left
    // unread makes the client's close a TCP reset, and the server loses the upload's tail.
    config.send_tls13_tickets = 0;
    (
        certificate.as_ref().to_vec(),
        TlsAcceptor::from(Arc::new(config)),
    )
}

/// A name as the wire carries it: UTF-8, or Latin-1 with `?` for what it cannot hold.
pub(crate) fn encode_name(name: &str, utf8: bool) -> Vec<u8> {
    if utf8 {
        return name.as_bytes().to_vec();
    }
    name.chars()
        .map(|character| u8::try_from(u32::from(character)).unwrap_or(b'?'))
        .collect()
}

/// `YYYYMMDDHHMMSS` of a file's modification time.
pub(crate) fn stamp(time: SystemTime) -> String {
    super::time::format_stamp(time).unwrap_or_else(|| "19700101000000".to_owned())
}

/// One listing line for `entry` in the `MLSD` or `LIST` dialect, without the line end. A
/// symbolic link is listed as one, with its target, as a Unix server lists it.
pub(crate) fn listing_line(entry: &Path, name: &str, mlsd: bool, utf8: bool) -> Vec<u8> {
    let Ok(metadata) = std::fs::symlink_metadata(entry) else {
        return Vec::new();
    };
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let is_dir = metadata.is_dir();
    let link = metadata
        .file_type()
        .is_symlink()
        .then(|| std::fs::read_link(entry).ok())
        .flatten()
        .map(|target| target.to_string_lossy().into_owned());
    let mut line = if mlsd {
        let slink;
        let kind = match name {
            "." => "cdir",
            ".." => "pdir",
            _ if is_dir => "dir",
            _ => match &link {
                Some(target) => {
                    slink = format!("OS.unix=slink:{target}");
                    slink.as_str()
                }
                None => "file",
            },
        };
        format!(
            "type={kind};size={};modify={};UNIX.mode={}; ",
            metadata.len(),
            stamp(modified),
            if is_dir { "0755" } else { "0644" }
        )
        .into_bytes()
    } else {
        let civil = Civil::from_system_time(modified).unwrap_or(Civil {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        });
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month = MONTHS[usize::try_from(civil.month - 1).unwrap_or(0)];
        format!(
            "{} 1 owner group {:>8} {month} {:02} {:>5} ",
            if is_dir {
                "drwxr-xr-x"
            } else if link.is_some() {
                "lrwxrwxrwx"
            } else {
                "-rw-r--r--"
            },
            if is_dir { 4096 } else { metadata.len() },
            civil.day,
            civil.year
        )
        .into_bytes()
    };
    line.extend(encode_name(name, utf8));
    if let (false, Some(target)) = (mlsd, &link) {
        line.extend(b" -> ");
        line.extend(encode_name(target, utf8));
    }
    line
}
