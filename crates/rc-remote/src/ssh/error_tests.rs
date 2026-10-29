use super::*;

fn offered() -> Box<OfferedKey> {
    Box::new(OfferedKey {
        algorithm: "ssh-ed25519".to_owned(),
        fingerprint: "SHA256:x".to_owned(),
        public_key: "AAAA".to_owned(),
    })
}

fn host_key(refusal: HostKeyRefusal) -> SshError {
    SshError::HostKey {
        host: "box".to_owned(),
        port: 22,
        offered: offered(),
        refusal,
    }
}

/// One error of every variant with a code of its own. The match stops compiling when a variant
/// is added: give it a sample here and its code in `ALL_CODES`.
fn one_of_each() -> Vec<SshError> {
    let samples = vec![
        SshError::NoUsername,
        SshError::Unreachable {
            host: "box".to_owned(),
            port: 22,
            reason: "refused".to_owned(),
        },
        SshError::Timeout {
            stage: Stage::Handshake,
            seconds: 15,
        },
        SshError::Cancelled,
        host_key(HostKeyRefusal::Unknown),
        host_key(HostKeyRefusal::Changed {
            stored_fingerprint: "SHA256:y".to_owned(),
        }),
        host_key(HostKeyRefusal::Failed {
            reason: "x".to_owned(),
        }),
        SshError::Credential(CredentialError::Locked),
        SshError::Key(PrivateKeyError::Unknown),
        SshError::Key(PrivateKeyError::PassphraseNeeded),
        SshError::Key(PrivateKeyError::PassphraseWrong),
        SshError::AuthFailed {
            tried: vec![AuthKind::Password],
        },
        SshError::ConnectionLost,
        SshError::Protocol {
            reason: "x".to_owned(),
        },
    ];
    for sample in &samples {
        match sample {
            SshError::NoUsername
            | SshError::Unreachable { .. }
            | SshError::Timeout { .. }
            | SshError::Cancelled
            | SshError::HostKey { .. }
            | SshError::HostKeyUnreadable(_)
            | SshError::Credential(_)
            | SshError::Key(_)
            | SshError::AuthFailed { .. }
            | SshError::ConnectionLost
            | SshError::Protocol { .. } => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_code_once() {
    let codes: Vec<&str> = one_of_each().iter().map(SshError::code).collect();
    assert_eq!(codes, SshError::ALL_CODES);
}

#[test]
fn a_credential_source_and_a_host_key_parse_keep_their_codes() {
    let failed = SshError::Credential(CredentialError::Failed {
        code: "vault_undecryptable",
        message: "x".to_owned(),
    });
    assert_eq!(failed.code(), "vault_undecryptable");
    let unreadable = SshError::HostKeyUnreadable(KeyError::Certificate);
    assert_eq!(unreadable.code(), KeyError::Certificate.code());
}

#[test]
fn a_dropped_transport_is_a_lost_connection() {
    for error in [
        russh::Error::Disconnect,
        russh::Error::HUP,
        russh::Error::KeepaliveTimeout,
    ] {
        assert_eq!(SshError::transport(&error).code(), CONNECTION_LOST);
    }
    assert_eq!(
        SshError::transport(&russh::Error::Version).code(),
        SSH_PROTOCOL
    );
}

#[test]
fn a_failed_login_names_what_was_tried_and_nothing_more() {
    let error = SshError::AuthFailed {
        tried: vec![AuthKind::Agent, AuthKind::Password],
    };
    assert_eq!(
        error.to_string(),
        "the SSH server accepted none of the methods tried (agent, password)"
    );
    let none = SshError::AuthFailed { tried: Vec::new() };
    assert!(none.to_string().contains("no method had"), "{none}");
}
