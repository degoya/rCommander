//! `FtpVfs` against the in-process test server: the `rc-vfs` conformance suite over plain FTP
//! (with `MLSD` and with `LIST` only), explicit and implicit FTPS; certificates, logins, names
//! in Latin-1 and UTF-8, and resuming a download.

use std::{path::Path, sync::Arc, time::Duration};

use rc_vfs::{Vfs, conformance};
use secrecy::SecretString;
use tokio::io::AsyncReadExt;

use super::FtpVfs;
use crate::ftp::{
    CertificateRejection, CertificateTrust, Credentials, Encryption, FtpConfig, PinnedCertificates,
    RefuseUnknown, TrustDecision, fingerprint,
    test_server::{Options, PASSWORD, ServerTls, TestServer},
};

/// Trusts exactly one certificate: the test server's. Test-only; the default refuses.
struct TrustExactly(Vec<u8>);

impl CertificateTrust for TrustExactly {
    fn decide(&self, rejection: &CertificateRejection) -> TrustDecision {
        if rejection.end_entity == self.0 {
            TrustDecision::Trust
        } else {
            TrustDecision::Refuse
        }
    }
}

pub(super) fn config(server: &TestServer, encryption: Encryption) -> FtpConfig {
    let mut config = FtpConfig::new(
        "127.0.0.1",
        encryption,
        Credentials::User {
            name: "tester".to_owned(),
            password: SecretString::from(PASSWORD),
        },
    );
    config.port = server.port;
    config.timeout = Duration::from_secs(10);
    config.trust = Arc::new(TrustExactly(server.certificate.clone()));
    config
}

async fn start(tls: ServerTls, mlst: bool) -> TestServer {
    TestServer::start(Options {
        tls,
        mlst,
        ..Options::default()
    })
    .await
}

/// The conformance suite in `/conformance` of a fresh server.
async fn conformance_over(tls: ServerTls, encryption: Encryption, mlst: bool) {
    let server = start(tls, mlst).await;
    std::fs::create_dir(server.local("conformance")).expect("root");
    let vfs = FtpVfs::connect(config(&server, encryption))
        .await
        .expect("connect");
    conformance::run(&vfs, Path::new("/conformance")).await;
}

#[tokio::test]
async fn plain_ftp_with_mlsd_conforms() {
    conformance_over(ServerTls::None, Encryption::None, true).await;
}

#[tokio::test]
async fn plain_ftp_with_list_only_conforms() {
    conformance_over(ServerTls::None, Encryption::None, false).await;
}

#[tokio::test]
async fn explicit_ftps_conforms() {
    conformance_over(ServerTls::Explicit, Encryption::Explicit, true).await;
}

#[tokio::test]
async fn implicit_ftps_conforms() {
    conformance_over(ServerTls::Implicit, Encryption::Implicit, true).await;
}

#[tokio::test]
async fn files_stream_between_two_connections() {
    let server = start(ServerTls::Explicit, true).await;
    for dir in ["from", "to"] {
        std::fs::create_dir(server.local(dir)).expect("dir");
    }
    let from = FtpVfs::connect(config(&server, Encryption::Explicit))
        .await
        .expect("connect");
    let to = FtpVfs::connect(config(&server, Encryption::Explicit))
        .await
        .expect("connect");
    conformance::copy_between(&from, Path::new("/from"), &to, Path::new("/to")).await;
}

#[tokio::test]
async fn an_unknown_ca_is_refused_by_default() {
    for (tls, encryption) in [
        (ServerTls::Explicit, Encryption::Explicit),
        (ServerTls::Implicit, Encryption::Implicit),
    ] {
        let server = start(tls, true).await;
        let mut refusing = config(&server, encryption);
        refusing.trust = Arc::new(RefuseUnknown);
        let error = FtpVfs::connect(refusing).await.expect_err("refused");
        assert_eq!(error.code(), "certificate_unknown", "{error}");
        let crate::ftp::FtpError::CertificateUnknown(rejection) = error else {
            panic!("not a certificate rejection");
        };
        assert_eq!(rejection.fingerprint, fingerprint(&server.certificate));
        assert_eq!(rejection.host, "127.0.0.1");
        assert_eq!(rejection.port, server.port);
        assert_eq!(
            server.passwords_received(),
            0,
            "no password leaves before the certificate is trusted"
        );
    }
}

/// RC-070-02: the certificate a person trusted for the server lets it in; another one stored
/// for the server refuses it before any password is sent.
#[tokio::test]
async fn a_pinned_certificate_is_trusted_and_another_pin_refuses_before_the_password() {
    for (tls, encryption) in [
        (ServerTls::Explicit, Encryption::Explicit),
        (ServerTls::Implicit, Encryption::Implicit),
    ] {
        let server = start(tls, true).await;
        let pin = |fingerprint_sha256: String| rc_db::TrustedCertificate {
            id: 1,
            host: "127.0.0.1".to_owned(),
            port: server.port,
            fingerprint_sha256,
            subject: String::new(),
            issuer: String::new(),
            not_after: i64::MAX,
            added_at: String::new(),
            last_seen_at: None,
        };
        let mut other = config(&server, encryption);
        other.trust = Arc::new(PinnedCertificates::new(
            Some(pin(fingerprint(b"another certificate"))),
            None,
        ));
        let error = FtpVfs::connect(other).await.expect_err("refused");
        assert_eq!(error.code(), "certificate_unknown", "{error}");
        assert_eq!(server.passwords_received(), 0);

        let mut pinned = config(&server, encryption);
        pinned.trust = Arc::new(PinnedCertificates::new(
            Some(pin(fingerprint(&server.certificate))),
            None,
        ));
        FtpVfs::connect(pinned).await.expect("trusted by its pin");
        assert_eq!(
            server.passwords_received(),
            1,
            "one login, after the handshake"
        );
    }
}

#[tokio::test]
async fn a_wrong_password_is_a_login_failure() {
    let server = start(ServerTls::None, true).await;
    let mut wrong = config(&server, Encryption::None);
    wrong.credentials = Credentials::User {
        name: "tester".to_owned(),
        password: SecretString::from("not-the-password"),
    };
    let error = FtpVfs::connect(wrong).await.expect_err("refused");
    assert_eq!(error.code(), "ftp_login_failed");
    assert!(!error.to_string().contains("not-the-password"));
}

#[tokio::test]
async fn explicit_ftps_against_a_plain_server_is_refused() {
    let server = start(ServerTls::None, true).await;
    let error = FtpVfs::connect(config(&server, Encryption::Explicit))
        .await
        .expect_err("no TLS");
    assert_eq!(error.code(), "ftp_tls_refused");
}

#[tokio::test]
async fn latin1_names_list_with_their_umlauts() {
    let server = TestServer::start(Options {
        utf8: false,
        mlst: false,
        ..Options::default()
    })
    .await;
    let name = "Gr\u{fc}\u{df}e.txt";
    std::fs::write(server.local(name), b"hallo").expect("file");
    let vfs = FtpVfs::connect(config(&server, Encryption::None))
        .await
        .expect("connect");

    let entries = vfs.list(Path::new("/")).await.expect("list");
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, [name]);
    assert_eq!(entries[0].size, 5);
    // The name cannot be sent back to a Latin-1 server through suppaftp: refused, not mangled.
    let code = vfs
        .open_read(&Path::new("/").join(name))
        .await
        .map(|_| ())
        .expect_err("unsupported")
        .code();
    assert_eq!(code, "unsupported");
}

#[tokio::test]
async fn utf8_names_round_trip() {
    let server = start(ServerTls::None, true).await;
    let name = "Gr\u{fc}\u{df}e.txt";
    std::fs::write(server.local(name), b"hallo").expect("file");
    let vfs = FtpVfs::connect(config(&server, Encryption::None))
        .await
        .expect("connect");
    let entries = vfs.list(Path::new("/")).await.expect("list");
    assert_eq!(entries[0].name, name);
    let path = Path::new("/").join(name);
    assert_eq!(conformance::read_file(&vfs, &path).await, b"hallo");
}

#[tokio::test]
async fn a_download_resumes_at_an_offset() {
    let server = start(ServerTls::None, true).await;
    std::fs::write(server.local("resume.bin"), b"0123456789").expect("file");
    let vfs = FtpVfs::connect(config(&server, Encryption::None))
        .await
        .expect("connect");
    let mut stream = vfs
        .open_read_at(Path::new("/resume.bin"), 4)
        .await
        .expect("open at 4");
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).await.expect("read");
    assert_eq!(rest, b"456789");
}
