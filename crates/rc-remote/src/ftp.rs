//! FTP and FTPS as a [`rc_vfs::Vfs`], on suppaftp with rustls (RC-070-01).
//!
//! [`FtpVfs::connect`] takes an [`FtpConfig`] - host, port, [`Encryption`] (none, explicit
//! `AUTH TLS`, implicit), [`DataMode`] (passive - `EPSV` or `PASV`, the other one when the
//! server refuses the first - or active), the
//! [`NameEncoding`] for servers without UTF-8, a timeout and the [`Credentials`] - logs in once
//! and keeps that connection for the calls that follow. Listings use `MLSD` where the server
//! offers it and `LIST` (Unix and DOS formats, `-a` unless refused) otherwise; `stat` uses
//! `MLST`, else `SIZE`,
//! `MDTM` and `CWD`. Reads stream with `RETR` (`REST` to resume, [`rc_vfs::Vfs::open_read_at`]),
//! writes with `STOR` and `APPE`.
//!
//! FTPS certificates are checked against the system roots, then the configuration's
//! [`CertificateTrust`] decides: by default it follows the roots (`certificate_unknown` for the
//! rest); [`PinnedCertificates`] adds the certificate a person trusted for the server, which is
//! the only one accepted from it from then on (RC-070-02).
//!
//! **Logging.** The password leaves only in `PASS`. suppaftp traces every control command at
//! `trace` level through the `log` facade, `PASS` included. rCommander's subscriber
//! (`rc_db::install_diagnostics`) bridges `log` into `tracing` and drops suppaftp's `trace`
//! lines whatever `RUST_LOG` says; any other subscriber caps the target at `debug`
//! ([`LOG_FILTER`]).

mod active;
mod config;
mod connection;
mod encoding;
mod error;
mod listing;
mod passive;
mod pool;
mod stream;
#[cfg(any(test, feature = "testkit"))]
pub mod test_server;
mod time;
mod tls;
mod trust;
mod vfs;

pub use config::{
    Credentials, DEFAULT_IMPLICIT_PORT, DEFAULT_PORT, DEFAULT_TIMEOUT, DEFAULT_TRANSFERS, DataMode,
    Encryption, FtpConfig, NameEncoding,
};
pub use error::{
    CERTIFICATE_UNKNOWN, FTP_CONNECT_FAILED, FTP_LOGIN_FAILED, FTP_PROTOCOL, FTP_TIMEOUT,
    FTP_TLS_FAILED, FTP_TLS_REFUSED, FTP_TOO_MANY_SESSIONS, FtpError, UNSUPPORTED_COMMANDS,
    unsupported_code,
};
pub use tls::{CertificateRejection, CertificateTrust, RefuseUnknown, TrustDecision, fingerprint};
pub use trust::{
    CertificateInfo, CertificateVerdict, PinnedCertificates, certificate_verdict,
    describe_certificate, unix_now,
};
pub use vfs::FtpVfs;

/// The filter directive a `log`/`tracing` subscriber must include: suppaftp's `trace` lines
/// carry the `PASS` command in clear.
pub const LOG_FILTER: &str = "suppaftp=debug";
