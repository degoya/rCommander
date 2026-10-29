//! The login against the in-process server: every method, the order, and what never leaks.

use std::{
    io,
    sync::{Arc, Mutex, PoisonError},
};

use russh::MethodKind;
use tokio_util::sync::CancellationToken;

use crate::{
    ssh::{AuthKind, AuthUsed, SessionOptions, SshError, SshSession, SshTarget},
    testkit::{
        CLIENT_ED25519, CLIENT_RSA_ENCRYPTED, CLIENT_RSA_PASSPHRASE, MemoryCredentials, MemoryGate,
        PASSWORD, ServerOptions, TestServer, client_ed25519, fingerprint, host_key_a,
    },
};

fn target(port: u16) -> SshTarget {
    SshTarget {
        host: "127.0.0.1".to_owned(),
        port,
        user: "tester".to_owned(),
    }
}

fn order(order: &[AuthKind]) -> SessionOptions {
    SessionOptions {
        auth_order: order.to_vec(),
        ..SessionOptions::default()
    }
}

async fn server() -> TestServer {
    TestServer::start(ServerOptions::new(host_key_a())).await
}

async fn login(
    server: &TestServer,
    options: &SessionOptions,
    credentials: &MemoryCredentials,
) -> Result<SshSession, SshError> {
    SshSession::connect(
        &target(server.port),
        options,
        Arc::new(MemoryGate::trusting(&host_key_a())),
        credentials,
        &CancellationToken::new(),
    )
    .await
}

fn with_password(password: &str) -> MemoryCredentials {
    MemoryCredentials {
        password: Some(password.to_owned()),
        ..MemoryCredentials::default()
    }
}

fn with_rsa_key() -> MemoryCredentials {
    MemoryCredentials {
        key: Some(CLIENT_RSA_ENCRYPTED.to_owned()),
        passphrase: Some(CLIENT_RSA_PASSPHRASE.to_owned()),
        ..MemoryCredentials::default()
    }
}

#[tokio::test]
async fn a_password_from_the_vault_logs_in() {
    let server = server().await;
    let session = login(
        &server,
        &SessionOptions::default(),
        &with_password(PASSWORD),
    )
    .await
    .expect("logged in");
    assert_eq!(session.auth(), &AuthUsed::Password);
    assert_eq!(session.host_key().fingerprint, fingerprint(&host_key_a()));
    assert!(
        server.auth_attempts() > 0,
        "the server counts what it is sent"
    );
}

#[tokio::test]
async fn a_key_with_its_passphrase_logs_in() {
    let server = server().await;
    let session = login(&server, &SessionOptions::default(), &with_rsa_key())
        .await
        .expect("logged in");
    assert!(
        matches!(session.auth(), AuthUsed::Key { fingerprint } if fingerprint.starts_with("SHA256:")),
        "{:?}",
        session.auth()
    );
}

#[tokio::test]
async fn a_key_whose_passphrase_is_wrong_says_so() {
    let server = server().await;
    let credentials = MemoryCredentials {
        passphrase: Some("not the passphrase".to_owned()),
        ..with_rsa_key()
    };
    let error = login(&server, &SessionOptions::default(), &credentials)
        .await
        .err()
        .expect("refused");
    assert_eq!(error.code(), "key_passphrase_wrong");
}

#[cfg(unix)]
#[tokio::test]
async fn an_agent_logs_in_without_asking_for_a_secret() {
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    let _agent = crate::testkit::start_agent(&socket, &[client_ed25519()]).await;
    let options = SessionOptions {
        agents: vec![crate::AgentSource::UnixSocket(socket)],
        ..SessionOptions::default()
    };
    let credentials = MemoryCredentials {
        locked: true,
        ..with_password(PASSWORD)
    };
    let session = login(&server, &options, &credentials)
        .await
        .expect("logged in");
    assert_eq!(
        session.auth(),
        &AuthUsed::Agent {
            source: crate::AgentSourceKind::SshAuthSock,
            fingerprint: fingerprint(&client_ed25519()),
        }
    );
    assert_eq!(credentials.asked(), 0, "the vault is never touched");
}

#[cfg(unix)]
#[tokio::test]
async fn an_agent_offers_only_the_chosen_key() {
    let server = server().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join("agent.sock");
    let _agent = crate::testkit::start_agent(&socket, &[client_ed25519()]).await;
    let options = SessionOptions {
        agents: vec![crate::AgentSource::UnixSocket(socket)],
        agent_fingerprint: Some("SHA256:another-key".to_owned()),
        ..order(&[AuthKind::Agent])
    };
    let error = login(&server, &options, &MemoryCredentials::default())
        .await
        .err()
        .expect("refused");
    assert!(
        matches!(&error, SshError::AuthFailed { tried } if tried.is_empty()),
        "{error:?}"
    );
}

#[tokio::test]
async fn the_order_decides_which_method_is_used() {
    let server = server().await;
    let both = MemoryCredentials {
        password: Some(PASSWORD.to_owned()),
        key: Some(CLIENT_ED25519.to_owned()),
        ..MemoryCredentials::default()
    };
    let session = login(&server, &order(&[AuthKind::Password, AuthKind::Key]), &both)
        .await
        .expect("logged in");
    assert_eq!(session.auth(), &AuthUsed::Password);
    let session = login(&server, &order(&[AuthKind::Key, AuthKind::Password]), &both)
        .await
        .expect("logged in");
    assert_eq!(
        session.auth(),
        &AuthUsed::Key {
            fingerprint: fingerprint(&client_ed25519())
        }
    );
}

#[tokio::test]
async fn a_server_that_asks_by_prompt_gets_the_password_there() {
    let server = TestServer::start(ServerOptions {
        methods: vec![MethodKind::KeyboardInteractive],
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let session = login(
        &server,
        &SessionOptions::default(),
        &with_password(PASSWORD),
    )
    .await
    .expect("logged in");
    assert_eq!(session.auth(), &AuthUsed::KeyboardInteractive);
}

#[tokio::test]
async fn a_wrong_password_fails_naming_the_method() {
    let server = server().await;
    let error = login(
        &server,
        &SessionOptions::default(),
        &with_password("wrong-password"),
    )
    .await
    .err()
    .expect("refused");
    assert!(
        matches!(&error, SshError::AuthFailed { tried } if *tried == [AuthKind::Password]),
        "{error:?}"
    );
    assert_eq!(error.code(), "ssh_auth_failed");
}

#[tokio::test]
async fn a_locked_vault_fails_with_vault_locked() {
    let server = server().await;
    let credentials = MemoryCredentials {
        locked: true,
        ..with_rsa_key()
    };
    let error = login(&server, &SessionOptions::default(), &credentials)
        .await
        .err()
        .expect("refused");
    assert_eq!(error.code(), "vault_locked");
}

#[tokio::test]
async fn a_login_without_a_user_is_refused_before_connecting() {
    let server = server().await;
    let target = SshTarget {
        user: "  ".to_owned(),
        ..target(server.port)
    };
    let error = SshSession::connect(
        &target,
        &SessionOptions::default(),
        Arc::new(MemoryGate::trusting(&host_key_a())),
        &with_password(PASSWORD),
        &CancellationToken::new(),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(error.code(), "ssh_no_username");
    assert_eq!(server.accepts(), 0);
}

/// Everything the log lines of this thread wrote.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl io::Write for Log {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn no_secret_reaches_a_log_line_or_an_error() {
    let log = Log::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let server = server().await;
    login(
        &server,
        &SessionOptions::default(),
        &with_password(PASSWORD),
    )
    .await
    .expect("logged in");
    login(&server, &SessionOptions::default(), &with_rsa_key())
        .await
        .expect("logged in");
    let wrong = login(
        &server,
        &SessionOptions::default(),
        &with_password("canary-wrong-password-0815"),
    )
    .await
    .err()
    .expect("refused");
    let logged =
        String::from_utf8_lossy(&log.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned();
    let seen = format!("{wrong} {wrong:?} {logged}");
    assert!(seen.contains("SSH login"), "the method is logged: {seen}");
    let key_line = CLIENT_RSA_ENCRYPTED.lines().nth(4).expect("a key line");
    for canary in [
        PASSWORD,
        "canary-wrong-password-0815",
        CLIENT_RSA_PASSPHRASE,
        key_line,
    ] {
        assert!(!seen.contains(canary), "{canary} leaked: {seen}");
    }
}

#[test]
fn the_default_order_needs_no_secret_for_the_agent() {
    assert_eq!(SessionOptions::default().auth_order[0], AuthKind::Agent);
}
