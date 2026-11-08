//! The login with OpenSSH user certificates (RC-060-09) against the in-process server with the
//! test CA as `TrustedUserCAKeys`: the server knows no key, only the CA.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::{
    CertificateError,
    ssh::{AuthKind, AuthUsed, SessionOptions, SshError, SshSession, SshTarget},
    testkit::{
        CLIENT_ED25519, CLIENT_ED25519_CERT, CLIENT_ED25519_CERT_EXPIRED,
        CLIENT_ED25519_CERT_OTHER_PRINCIPAL, CLIENT_RSA_CERT, CLIENT_RSA_ENCRYPTED,
        CLIENT_RSA_PASSPHRASE, HOST_KEY_B_CERT, MemoryCredentials, MemoryGate, ServerOptions,
        TestServer, client_ed25519, fingerprint, host_key_a, user_ca,
    },
};

/// A server that trusts the test CA and no key of its own.
async fn ca_server() -> TestServer {
    TestServer::start(ServerOptions {
        authorized: Vec::new(),
        user_ca: Some(user_ca()),
        ..ServerOptions::new(host_key_a())
    })
    .await
}

fn key_only() -> SessionOptions {
    SessionOptions {
        auth_order: vec![AuthKind::Key],
        ..SessionOptions::default()
    }
}

fn with_certificate(certificate: &str) -> MemoryCredentials {
    MemoryCredentials {
        key: Some(CLIENT_ED25519.to_owned()),
        certificate: Some(certificate.to_owned()),
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

#[tokio::test]
async fn a_certificate_the_ca_signed_logs_in() {
    let server = ca_server().await;
    let session = login(&server, &key_only(), &with_certificate(CLIENT_ED25519_CERT))
        .await
        .expect("logged in");
    assert_eq!(
        session.auth(),
        &AuthUsed::Key {
            fingerprint: fingerprint(&client_ed25519()),
            certificate: Some("rc-test-valid".to_owned()),
        }
    );
}

#[tokio::test]
async fn an_rsa_certificate_signs_with_the_hash_it_names() {
    let server = ca_server().await;
    let credentials = MemoryCredentials {
        key: Some(CLIENT_RSA_ENCRYPTED.to_owned()),
        passphrase: Some(CLIENT_RSA_PASSPHRASE.to_owned()),
        certificate: Some(CLIENT_RSA_CERT.to_owned()),
        ..MemoryCredentials::default()
    };
    let session = login(&server, &key_only(), &credentials)
        .await
        .expect("logged in");
    assert!(
        matches!(session.auth(), AuthUsed::Key { certificate: Some(id), .. } if id == "rc-test-rsa"),
        "{:?}",
        session.auth()
    );
}

#[tokio::test]
async fn an_expired_certificate_is_not_sent_and_says_so() {
    let server = ca_server().await;
    let error = login(
        &server,
        &key_only(),
        &with_certificate(CLIENT_ED25519_CERT_EXPIRED),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(error.code(), "ssh_certificate_expired");
    assert!(
        matches!(
            &error,
            SshError::Certificate(CertificateError::Expired { key_id, .. }) if key_id == "rc-test-expired"
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains("expired"), "{error}");
}

#[tokio::test]
async fn a_certificate_for_another_principal_is_refused_naming_both() {
    let server = ca_server().await;
    let error = login(
        &server,
        &key_only(),
        &with_certificate(CLIENT_ED25519_CERT_OTHER_PRINCIPAL),
    )
    .await
    .err()
    .expect("refused");
    assert_eq!(error.code(), "ssh_certificate_principal");
    let message = error.to_string();
    assert!(
        message.contains("someone-else") && message.contains("tester"),
        "{message}"
    );
}

#[tokio::test]
async fn another_keys_certificate_is_not_sent() {
    let server = ca_server().await;
    let error = login(&server, &key_only(), &with_certificate(HOST_KEY_B_CERT))
        .await
        .err()
        .expect("refused");
    assert_eq!(error.code(), "ssh_certificate_key_mismatch");
}

#[tokio::test]
async fn with_certificates_off_the_key_logs_in_alone() {
    let server = ca_server().await;
    let options = SessionOptions {
        use_certificates: false,
        ..key_only()
    };
    let error = login(&server, &options, &with_certificate(CLIENT_ED25519_CERT))
        .await
        .err()
        .expect("refused: the server knows no key");
    assert!(
        matches!(&error, SshError::AuthFailed { tried } if *tried == [AuthKind::Key]),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_server_that_knows_the_key_still_takes_it_after_a_refused_certificate() {
    let server = TestServer::start(ServerOptions {
        user_ca: Some(user_ca()),
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let session = login(
        &server,
        &key_only(),
        &with_certificate(CLIENT_ED25519_CERT_OTHER_PRINCIPAL),
    )
    .await
    .expect("logged in with the key");
    assert_eq!(
        session.auth(),
        &AuthUsed::Key {
            fingerprint: fingerprint(&client_ed25519()),
            certificate: None,
        }
    );
}

#[cfg(unix)]
mod agent {
    use super::*;
    use crate::{
        AgentSource, AgentSourceKind,
        testkit::{cert_agent::start_cert_agent, certificate},
    };

    fn agent_only(socket: std::path::PathBuf) -> SessionOptions {
        SessionOptions {
            auth_order: vec![AuthKind::Agent],
            agents: vec![AgentSource::UnixSocket(socket)],
            ..SessionOptions::default()
        }
    }

    #[tokio::test]
    async fn an_agents_certificate_is_used_instead_of_skipped() {
        let server = ca_server().await;
        let dir = tempfile::tempdir().expect("temp dir");
        let socket = dir.path().join("agent.sock");
        let _agent = start_cert_agent(
            &socket,
            &[(client_ed25519(), Some(certificate(CLIENT_ED25519_CERT)))],
        );
        let session = login(&server, &agent_only(socket), &MemoryCredentials::default())
            .await
            .expect("logged in");
        assert_eq!(
            session.auth(),
            &AuthUsed::Agent {
                source: AgentSourceKind::SshAuthSock,
                fingerprint: fingerprint(&client_ed25519()),
                certificate: Some("rc-test-valid".to_owned()),
            }
        );
    }

    #[tokio::test]
    async fn an_agents_expired_certificate_is_passed_over() {
        let server = ca_server().await;
        let dir = tempfile::tempdir().expect("temp dir");
        let socket = dir.path().join("agent.sock");
        let _agent = start_cert_agent(
            &socket,
            &[(
                client_ed25519(),
                Some(certificate(CLIENT_ED25519_CERT_EXPIRED)),
            )],
        );
        let error = login(&server, &agent_only(socket), &MemoryCredentials::default())
            .await
            .err()
            .expect("refused");
        assert!(
            matches!(&error, SshError::AuthFailed { tried } if *tried == [AuthKind::Agent]),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn an_agents_certificate_for_another_principal_says_so() {
        let server = ca_server().await;
        let dir = tempfile::tempdir().expect("temp dir");
        let socket = dir.path().join("agent.sock");
        let _agent = start_cert_agent(
            &socket,
            &[(
                client_ed25519(),
                Some(certificate(CLIENT_ED25519_CERT_OTHER_PRINCIPAL)),
            )],
        );
        let error = login(&server, &agent_only(socket), &MemoryCredentials::default())
            .await
            .err()
            .expect("refused");
        assert_eq!(error.code(), "ssh_certificate_principal");
    }

    #[tokio::test]
    async fn with_certificates_off_an_agent_offers_only_its_keys() {
        let server = ca_server().await;
        let dir = tempfile::tempdir().expect("temp dir");
        let socket = dir.path().join("agent.sock");
        let _agent = start_cert_agent(
            &socket,
            &[(client_ed25519(), Some(certificate(CLIENT_ED25519_CERT)))],
        );
        let options = SessionOptions {
            use_certificates: false,
            ..agent_only(socket)
        };
        let error = login(&server, &options, &MemoryCredentials::default())
            .await
            .err()
            .expect("refused: the server knows no key");
        assert!(
            matches!(&error, SshError::AuthFailed { tried } if *tried == [AuthKind::Agent]),
            "{error:?}"
        );
    }
}
