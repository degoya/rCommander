//! What it takes to reach one FTP server: address, encryption, data connection mode, name
//! encoding, timeout and credentials.
//!
//! The password arrives here as a [`SecretString`] from whoever holds it - the vault once
//! RC-050-03 stores FTP secrets, a test until then - and leaves only in the `PASS` command. It is
//! never part of a `Debug` output or an error.

use std::{fmt, sync::Arc, time::Duration};

use secrecy::SecretString;

use super::tls::{CertificateTrust, RefuseUnknown};

/// The FTP control port.
pub const DEFAULT_PORT: u16 = 21;
/// The port of implicit FTPS.
pub const DEFAULT_IMPLICIT_PORT: u16 = 990;
/// How long connecting, logging in and every single command may take.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// How many transfers to one server run at once, each on a control connection of its own
/// (RC-070-03); listings and other commands have one more.
pub const DEFAULT_TRANSFERS: usize = 2;

/// Anonymous FTP convention: the user name is fixed and the password is a contact address. A
/// real-looking address would be someone's data, so a placeholder goes out instead.
pub(crate) const ANONYMOUS_USER: &str = "anonymous";
pub(crate) const ANONYMOUS_PASSWORD: &str = "anonymous@example.invalid";

/// Whether and how the connection is encrypted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Encryption {
    /// Plain FTP: commands, password and data in clear.
    None,
    /// Explicit FTPS: `AUTH TLS` on the normal port, then `PROT P` for the data connections.
    #[default]
    Explicit,
    /// Implicit FTPS: TLS from the first byte, usually on port 990.
    Implicit,
}

/// How data connections are opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DataMode {
    /// The client connects to the server (`EPSV` where offered, `PASV` otherwise).
    #[default]
    Passive,
    /// The server connects back to the client (`PORT`/`EPRT`).
    Active,
}

/// How names are encoded on a server that does not offer UTF-8 (`FEAT` without `UTF8`). A
/// server that offers it always gets `OPTS UTF8 ON` and UTF-8.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NameEncoding {
    /// ISO 8859-1, what old Unix and NAS servers mostly send.
    #[default]
    Latin1,
    /// UTF-8 although the server does not announce it.
    Utf8,
}

/// Who logs in.
#[derive(Clone)]
pub enum Credentials {
    /// `anonymous` with a placeholder address as password.
    Anonymous,
    /// A user name and its password.
    User {
        name: String,
        password: SecretString,
    },
}

impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anonymous => formatter.write_str("Anonymous"),
            Self::User { name, .. } => formatter
                .debug_struct("User")
                .field("name", name)
                .field("password", &"<redacted>")
                .finish(),
        }
    }
}

/// One FTP server and how to talk to it.
#[derive(Clone)]
pub struct FtpConfig {
    /// Host name or address, as the certificate names it.
    pub host: String,
    pub port: u16,
    pub encryption: Encryption,
    pub data_mode: DataMode,
    /// The encoding of names when the server does not offer UTF-8.
    pub encoding: NameEncoding,
    /// The limit for connecting and logging in, and for every command.
    pub timeout: Duration,
    /// How many transfers run at once ([`DEFAULT_TRANSFERS`]); one more waits for a free one.
    /// At least one: zero counts as one.
    pub transfers: usize,
    /// A listing looks up what its links point at (at most 64, after the transfer, sent last).
    /// Off, the default, links are listed as links and looked up when one is stat-ed; what was
    /// learned is kept for the session (RC-080-46).
    pub resolve_links: bool,
    pub credentials: Credentials,
    /// Decides on a certificate the system roots do not vouch for. The default refuses it
    /// (`certificate_unknown`); RC-070-02 brings the stored decisions.
    pub trust: Arc<dyn CertificateTrust>,
}

impl FtpConfig {
    /// A configuration with the defaults: explicit FTPS, passive mode, Latin-1 fallback, 30 s,
    /// two transfers at once, links looked up lazily, unknown certificates refused. The port
    /// follows the encryption.
    #[must_use]
    pub fn new(host: impl Into<String>, encryption: Encryption, credentials: Credentials) -> Self {
        Self {
            host: host.into(),
            port: match encryption {
                Encryption::Implicit => DEFAULT_IMPLICIT_PORT,
                Encryption::None | Encryption::Explicit => DEFAULT_PORT,
            },
            encryption,
            data_mode: DataMode::default(),
            encoding: NameEncoding::default(),
            timeout: DEFAULT_TIMEOUT,
            transfers: DEFAULT_TRANSFERS,
            resolve_links: false,
            credentials,
            trust: Arc::new(RefuseUnknown),
        }
    }

    /// The user name and password that go out in `USER` and `PASS`.
    pub(crate) fn login(&self) -> (&str, &SecretString) {
        static ANONYMOUS: std::sync::OnceLock<SecretString> = std::sync::OnceLock::new();
        match &self.credentials {
            Credentials::Anonymous => (
                ANONYMOUS_USER,
                ANONYMOUS.get_or_init(|| SecretString::from(ANONYMOUS_PASSWORD)),
            ),
            Credentials::User { name, password } => (name, password),
        }
    }
}

impl fmt::Debug for FtpConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("encryption", &self.encryption)
            .field("data_mode", &self.data_mode)
            .field("encoding", &self.encoding)
            .field("timeout", &self.timeout)
            .field("transfers", &self.transfers)
            .field("resolve_links", &self.resolve_links)
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
