//! Test material for SSH sessions: this crate's tests use it, and the engine's with the feature
//! `testkit`. An in-process server ([`server`], its commands [`exec`]) with an SFTP subsystem on
//! a directory ([`SftpServe`]), one with a pretend shell for the terminal ([`shell`]), the
//! throw-away keys of `tests/fixtures` with the user certificates a test CA signed for them
//! (`tests/fixtures/certs`, RC-060-09), a gate and credentials that answer from memory, and on
//! Unix an SSH agent on a socket - russh's, or one that also holds certificates
//! ([`cert_agent`]).
//!
//! Nothing here belongs in a release build: the keys are public fixtures and the gate trusts what
//! it is told to.

#[cfg(unix)]
pub mod cert_agent;
pub mod exec;
pub mod server;
mod sftp;
pub mod shell;

use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use russh::keys::{Certificate, HashAlg, PrivateKey, PublicKey, decode_secret_key};
use secrecy::{ExposeSecret as _, SecretString};

pub use exec::ExecRuns;
pub use server::{SecondFactor, ServerOptions, TestServer, echo_server, silent_server};
pub use sftp::SftpServe;
pub use shell::{PtyRequest, SUDO_PROMPT, ShellServer, shell_word};

use crate::{
    OfferedKey,
    ssh::{CredentialError, Credentials, HostKeyGate, HostKeyRefusal, KeyMaterial},
};

/// The password the test server takes; long enough to be scrubbed from MCP answers.
pub const PASSWORD: &str = "canary-password-4711";
/// An ed25519 client key without a passphrase.
pub const CLIENT_ED25519: &str = include_str!("../tests/fixtures/client_key_ed25519");
/// An RSA client key in PEM, encrypted with [`CLIENT_RSA_PASSPHRASE`].
pub const CLIENT_RSA_ENCRYPTED: &str = include_str!("../tests/fixtures/client_key_rsapem");
/// The passphrase of [`CLIENT_RSA_ENCRYPTED`].
pub const CLIENT_RSA_PASSPHRASE: &str = "fixture-passphrase";

/// The test CA's public key; its private half was thrown away once it signed the certificates
/// below (`ssh-keygen -s`).
pub const USER_CA: &str = include_str!("../tests/fixtures/certs/user_ca.pub");
/// A user certificate of [`CLIENT_ED25519`] for the principal `tester`, valid forever, key id
/// `rc-test-valid`.
pub const CLIENT_ED25519_CERT: &str =
    include_str!("../tests/fixtures/certs/client_ed25519-cert.pub");
/// As [`CLIENT_ED25519_CERT`], valid only on 2020-01-01, key id `rc-test-expired`.
pub const CLIENT_ED25519_CERT_EXPIRED: &str =
    include_str!("../tests/fixtures/certs/client_ed25519-expired-cert.pub");
/// As [`CLIENT_ED25519_CERT`], for the principal `someone-else`, key id
/// `rc-test-other-principal`.
pub const CLIENT_ED25519_CERT_OTHER_PRINCIPAL: &str =
    include_str!("../tests/fixtures/certs/client_ed25519-other-principal-cert.pub");
/// A user certificate of [`CLIENT_RSA_ENCRYPTED`] for `tester`, valid forever, key id
/// `rc-test-rsa`.
pub const CLIENT_RSA_CERT: &str = include_str!("../tests/fixtures/certs/client_rsa-cert.pub");
/// A user certificate of [`host_key_b`]'s key for `tester`: another key's.
pub const HOST_KEY_B_CERT: &str = include_str!("../tests/fixtures/certs/host_key_b-cert.pub");

const HOST_KEY_A: &str = include_str!("../tests/fixtures/host_key_a");
const HOST_KEY_B: &str = include_str!("../tests/fixtures/host_key_b");

/// The server identity the tests use.
///
/// # Panics
///
/// The fixture does not parse.
#[must_use]
pub fn host_key_a() -> PrivateKey {
    decode_secret_key(HOST_KEY_A, None).expect("fixture host key a")
}

/// A second identity: the same server rebuilt, or an impostor.
///
/// # Panics
///
/// The fixture does not parse.
#[must_use]
pub fn host_key_b() -> PrivateKey {
    decode_secret_key(HOST_KEY_B, None).expect("fixture host key b")
}

/// The ed25519 client key, opened.
///
/// # Panics
///
/// The fixture does not parse.
#[must_use]
pub fn client_ed25519() -> PrivateKey {
    decode_secret_key(CLIENT_ED25519, None).expect("fixture client key")
}

/// The test CA's public key, what a server with `TrustedUserCAKeys` trusts.
///
/// # Panics
///
/// The fixture does not parse.
#[must_use]
pub fn user_ca() -> PublicKey {
    PublicKey::from_openssh(USER_CA).expect("fixture user CA")
}

/// The certificate of the fixture `text`.
///
/// # Panics
///
/// The fixture does not parse.
#[must_use]
pub fn certificate(text: &str) -> Certificate {
    Certificate::from_openssh(text.trim()).expect("fixture certificate")
}

/// The public halves of both client keys, what the test server accepts.
///
/// # Panics
///
/// A fixture does not parse.
#[must_use]
pub fn client_public_keys() -> Vec<PublicKey> {
    let rsa = decode_secret_key(CLIENT_RSA_ENCRYPTED, Some(CLIENT_RSA_PASSPHRASE))
        .expect("fixture rsa key");
    vec![
        client_ed25519().public_key().clone(),
        rsa.public_key().clone(),
    ]
}

/// The `SHA256:` fingerprint of `key`'s public half.
#[must_use]
pub fn fingerprint(key: &PrivateKey) -> String {
    key.public_key().fingerprint(HashAlg::Sha256).to_string()
}

/// A gate that trusts the fingerprints it is given: any other key is unknown when it trusts
/// none, changed when it trusts another.
#[derive(Default)]
pub struct MemoryGate {
    trusted: Vec<String>,
    seen: Mutex<Vec<OfferedKey>>,
}

impl MemoryGate {
    /// Trusts exactly `key`.
    #[must_use]
    pub fn trusting(key: &PrivateKey) -> Self {
        Self {
            trusted: vec![fingerprint(key)],
            seen: Mutex::default(),
        }
    }

    /// The keys it was asked about.
    #[must_use]
    pub fn seen(&self) -> Vec<OfferedKey> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl HostKeyGate for MemoryGate {
    async fn check(
        &self,
        _host: &str,
        _port: u16,
        offered: OfferedKey,
    ) -> Result<(), HostKeyRefusal> {
        let trusted = self.trusted.contains(&offered.fingerprint);
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(offered);
        if trusted {
            return Ok(());
        }
        Err(self
            .trusted
            .first()
            .map_or(HostKeyRefusal::Unknown, |stored| HostKeyRefusal::Changed {
                stored_fingerprint: stored.clone(),
            }))
    }
}

/// Secrets from memory, counting how often one was asked for. `locked` answers like a locked
/// vault for every secret that is set.
#[derive(Default)]
pub struct MemoryCredentials {
    pub password: Option<String>,
    pub key: Option<String>,
    pub passphrase: Option<String>,
    /// The key's certificate (RC-060-09), its `-cert.pub` line.
    pub certificate: Option<String>,
    /// A one-time code seed (RC-060-08): Base32 or an `otpauth://` URI.
    pub totp_seed: Option<String>,
    pub locked: bool,
    /// How many secrets were asked for; public so a test can build one with `..default()`.
    pub asks: AtomicUsize,
}

impl MemoryCredentials {
    /// How many secrets were asked for.
    #[must_use]
    pub fn asked(&self) -> usize {
        self.asks.load(Ordering::SeqCst)
    }

    fn give(&self, value: Option<&str>) -> Result<Option<SecretString>, CredentialError> {
        self.asks.fetch_add(1, Ordering::SeqCst);
        match value {
            Some(_) if self.locked => Err(CredentialError::Locked),
            value => Ok(value.map(SecretString::from)),
        }
    }
}

#[async_trait]
impl Credentials for MemoryCredentials {
    async fn password(&self) -> Result<Option<SecretString>, CredentialError> {
        self.give(self.password.as_deref())
    }

    async fn private_key(&self) -> Result<Option<KeyMaterial>, CredentialError> {
        let Some(key) = self.give(self.key.as_deref())? else {
            return Ok(None);
        };
        let passphrase = self.passphrase.as_deref().map(SecretString::from);
        Ok(Some(KeyMaterial {
            key,
            passphrase,
            certificate: self.certificate.clone(),
        }))
    }

    async fn totp_code(&self) -> Result<Option<SecretString>, CredentialError> {
        let Some(seed) = self.give(self.totp_seed.as_deref())? else {
            return Ok(None);
        };
        let seed = crate::totp::TotpSeed::parse(seed.expose_secret()).map_err(|error| {
            CredentialError::Failed {
                code: error.code(),
                message: error.to_string(),
            }
        })?;
        Ok(Some(seed.code_now()))
    }
}

/// An SSH agent holding `keys`, listening on the Unix socket `path`, for as long as the task
/// runs.
///
/// # Panics
///
/// The socket cannot be bound or a key cannot be added.
#[cfg(unix)]
pub async fn start_agent(
    path: &std::path::Path,
    keys: &[PrivateKey],
) -> tokio::task::JoinHandle<()> {
    use russh::keys::agent::client::AgentClient;
    use tokio::net::UnixListener;

    let listener = UnixListener::bind(path).expect("bind the agent socket");
    let connections = Box::pin(futures_util::stream::unfold(
        listener,
        |listener| async move {
            let accepted = listener.accept().await.map(|(stream, _)| stream);
            Some((accepted, listener))
        },
    ));
    let task = tokio::spawn(async move {
        let _ = russh::keys::agent::server::serve(connections, ()).await;
    });
    let mut client = AgentClient::connect_uds(path)
        .await
        .expect("open the agent");
    for key in keys {
        client.add_identity(key, &[]).await.expect("add a key");
    }
    task
}
