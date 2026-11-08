use super::*;

fn certificate() -> KeyCertificate {
    KeyCertificate {
        cert_type: "ssh-ed25519-cert-v01@openssh.com".into(),
        key_id: "rc-test-valid".into(),
        serial: 1,
        principals: vec!["tester".into(), "deploy".into()],
        valid_after: 100,
        valid_before: Some(200),
        ca_fingerprint: "SHA256:ca".into(),
        openssh: String::new(),
    }
}

#[test]
fn a_valid_certificate_names_its_id_principals_and_validity() {
    let lines = lines(&certificate(), 150);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].0.contains("rc-test-valid"), "{lines:?}");
    assert!(lines[0].0.contains("tester, deploy"), "{lines:?}");
    assert!(lines.iter().all(|(_, warning)| !warning));
}

#[test]
fn an_expired_certificate_warns() {
    let lines = lines(&certificate(), 200);
    assert_eq!(
        lines.last(),
        Some(&(t!("connections.ssh_certificate_expired").to_owned(), true))
    );
}

#[test]
fn a_certificate_not_yet_valid_warns() {
    let lines = lines(&certificate(), 99);
    assert_eq!(
        lines.last(),
        Some(&(
            t!("connections.ssh_certificate_not_yet_valid").to_owned(),
            true
        ))
    );
}

#[test]
fn an_open_certificate_says_so() {
    let open = KeyCertificate {
        principals: Vec::new(),
        valid_after: 0,
        valid_before: None,
        ..certificate()
    };
    let lines = lines(&open, 150);
    assert!(
        lines[0].0.contains(t!("connections.ssh_certificate_any")),
        "{lines:?}"
    );
    assert_eq!(lines[1].0, t!("connections.ssh_certificate_valid_open"));
    let until = KeyCertificate {
        valid_before: Some(200),
        ..open
    };
    assert_eq!(
        lines_at(&until)[1].0,
        t!("connections.ssh_certificate_valid_until", until = date(200))
    );
}

fn lines_at(certificate: &KeyCertificate) -> Vec<(String, bool)> {
    lines(certificate, 150)
}
