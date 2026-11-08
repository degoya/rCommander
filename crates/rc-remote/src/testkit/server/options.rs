//! What the test server accepts and offers ([`ServerOptions`]).

use russh::{
    MethodKind,
    keys::{PrivateKey, PublicKey},
};

use super::super as testkit;
use super::SecondFactor;

/// What the server accepts and offers.
#[derive(Clone)]
pub struct ServerOptions {
    pub host_key: PrivateKey,
    /// The password accepted for every user; none when `None`.
    pub password: Option<String>,
    /// The public keys accepted for every user.
    pub authorized: Vec<PublicKey>,
    /// The methods offered to the client.
    pub methods: Vec<MethodKind>,
    /// After the login, try to open channels towards the client ([`super::TestServer::probes`]).
    pub probe_forwarding: bool,
    /// The `sftp` subsystem; refused when `None`.
    pub sftp: Option<testkit::SftpServe>,
    /// A one-time code `keyboard-interactive` asks for after the password (RC-060-08).
    pub second_factor: Option<SecondFactor>,
    /// Carry `direct-tcpip` channels to ports of 127.0.0.1 (RC-110-03); refused when false.
    pub direct_tcpip: bool,
    /// The CA whose user certificates are accepted, as `TrustedUserCAKeys` does (RC-060-09):
    /// signed by it, valid now and naming the user among its principals.
    pub user_ca: Option<PublicKey>,
}

impl ServerOptions {
    /// A server with `host_key` that takes [`testkit::PASSWORD`] and the fixtures' client keys over
    /// `password`, `publickey` and `keyboard-interactive`.
    #[must_use]
    pub fn new(host_key: PrivateKey) -> Self {
        Self {
            host_key,
            password: Some(testkit::PASSWORD.to_owned()),
            authorized: testkit::client_public_keys(),
            methods: vec![
                MethodKind::Password,
                MethodKind::PublicKey,
                MethodKind::KeyboardInteractive,
            ],
            probe_forwarding: false,
            sftp: None,
            second_factor: None,
            direct_tcpip: false,
            user_ca: None,
        }
    }
}
