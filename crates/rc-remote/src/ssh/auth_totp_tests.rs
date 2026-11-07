//! Password and one-time code over `keyboard-interactive` against the in-process server, as
//! `sshd` with Google Authenticator's PAM module asks for them (RC-060-08).

use std::{
    io,
    sync::{Arc, Mutex, PoisonError},
};

use russh::MethodKind;
use secrecy::ExposeSecret as _;
use tokio_util::sync::CancellationToken;

use crate::{
    login_script::Pattern,
    ssh::{AuthKind, AuthUsed, SessionOptions, SshError, SshSession, SshTarget},
    testkit::{
        MemoryCredentials, MemoryGate, PASSWORD, SecondFactor, ServerOptions, TestServer,
        host_key_a,
    },
    totp::{TotpSeed, unix_now},
};

/// A seed that is easy to spot wherever it leaks: 32 Base32 symbols, 20 bytes.
const SEED: &str = "CANARYTOTPSEEDA2CANARYTOTPSEEDB3";

/// A server that takes the password over `keyboard-interactive` only, then asks for the code.
async fn two_factor_server() -> TestServer {
    TestServer::start(ServerOptions {
        methods: vec![MethodKind::KeyboardInteractive],
        second_factor: Some(SecondFactor::verification_code(SEED)),
        ..ServerOptions::new(host_key_a())
    })
    .await
}

/// The password method alone: no agent or key is asked first.
fn options() -> SessionOptions {
    SessionOptions {
        auth_order: vec![AuthKind::Password],
        ..SessionOptions::default()
    }
}

fn credentials(seed: Option<&str>) -> MemoryCredentials {
    MemoryCredentials {
        password: Some(PASSWORD.to_owned()),
        totp_seed: seed.map(str::to_owned),
        ..MemoryCredentials::default()
    }
}

async fn login(
    server: &TestServer,
    options: &SessionOptions,
    credentials: &MemoryCredentials,
) -> Result<SshSession, SshError> {
    let target = SshTarget {
        host: "127.0.0.1".to_owned(),
        port: server.port,
        user: "tester".to_owned(),
    };
    SshSession::connect(
        &target,
        options,
        Arc::new(MemoryGate::trusting(&host_key_a())),
        credentials,
        &CancellationToken::new(),
    )
    .await
}

fn refused_by_password(error: &SshError) -> bool {
    matches!(error, SshError::AuthFailed { tried } if *tried == [AuthKind::Password])
}

#[tokio::test]
async fn password_and_code_log_in_over_keyboard_interactive() {
    let server = two_factor_server().await;
    let credentials = credentials(Some(SEED));
    let session = login(&server, &options(), &credentials)
        .await
        .expect("logged in with password and code");
    assert_eq!(session.auth(), &AuthUsed::KeyboardInteractive);
    assert_eq!(credentials.asked(), 2, "the password, then the code");
}

#[tokio::test]
async fn an_otpauth_uri_seed_logs_in_too() {
    let server = two_factor_server().await;
    let uri = format!("otpauth://totp/Example:tester?secret={SEED}&issuer=Example");
    let session = login(&server, &options(), &credentials(Some(uri.as_str())))
        .await
        .expect("logged in");
    assert_eq!(session.auth(), &AuthUsed::KeyboardInteractive);
}

#[tokio::test]
async fn a_question_the_pattern_misses_goes_to_the_person() {
    let server = two_factor_server().await;
    let options = SessionOptions {
        totp_prompt: Pattern::new("(?i)passcode").expect("compiles"),
        ..options()
    };
    let credentials = credentials(Some(SEED));
    let error = login(&server, &options, &credentials)
        .await
        .err()
        .expect("not answered");
    assert!(refused_by_password(&error), "{error:?}");
    assert_eq!(credentials.asked(), 1, "the seed is never read");
}

#[tokio::test]
async fn without_a_seed_the_code_is_not_guessed() {
    let server = two_factor_server().await;
    let error = login(&server, &options(), &credentials(None))
        .await
        .err()
        .expect("not answered");
    assert!(refused_by_password(&error), "{error:?}");
}

#[tokio::test]
async fn a_wrong_seed_is_refused_by_the_server() {
    let server = two_factor_server().await;
    let error = login(
        &server,
        &options(),
        &credentials(Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")),
    )
    .await
    .err()
    .expect("refused");
    assert!(refused_by_password(&error), "{error:?}");
}

#[tokio::test]
async fn a_seed_alone_answers_and_a_locked_vault_says_so() {
    let server = two_factor_server().await;
    let credentials = MemoryCredentials {
        locked: true,
        totp_seed: Some(SEED.to_owned()),
        ..MemoryCredentials::default()
    };
    let error = login(&server, &options(), &credentials)
        .await
        .err()
        .expect("refused");
    assert_eq!(error.code(), "vault_locked");
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
async fn neither_seed_nor_code_reaches_a_log_line_or_an_error() {
    let log = Log::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .without_time()
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let server = two_factor_server().await;
    let before = unix_now();
    login(&server, &options(), &credentials(Some(SEED)))
        .await
        .expect("logged in");
    let wrong = login(
        &server,
        &options(),
        &credentials(Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")),
    )
    .await
    .err()
    .expect("refused");
    let after = unix_now();
    let logged =
        String::from_utf8_lossy(&log.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned();
    let seen = format!("{wrong} {wrong:?} {logged}");
    assert!(seen.contains("one-time code"), "the step is logged: {seen}");
    let seed = TotpSeed::parse(SEED).expect("valid");
    let mut canaries = vec![SEED.to_owned()];
    for at in [before, after] {
        canaries.push(seed.code_at(at).expose_secret().to_owned());
    }
    for canary in canaries {
        assert!(!seen.contains(&canary), "{canary} leaked: {seen}");
    }
}
