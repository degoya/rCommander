use super::*;
use crate::pem;
use crate::test_support::fixture;

fn text(name: &str) -> &'static str {
    std::str::from_utf8(fixture(name)).expect("text")
}

#[test]
fn a_public_key_line() {
    let line = key_line(text("id_ed25519.pub")).expect("a key");
    assert_eq!(line.kind, "ssh-ed25519");
    assert_eq!(
        line.fingerprint,
        "SHA256:h4aqM5BLHTKRyRe0Wf93JkqavgnAcEW4PY9jbEUawWI"
    );
    assert_eq!(line.comment.as_deref(), Some("test@example"));
    assert_eq!(line.options, None);
    assert!(matches!(line.key, Some(PublicKey::Ed25519(ref key)) if key.len() == 32));
}

#[test]
fn authorized_keys_options_with_quoted_spaces() {
    let line = key_line(
        "command=\"/usr/bin/echo a b\",no-pty ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBFbG ok",
    );
    // The blob above is cut short: the key type matches, the point is missing.
    assert!(line.is_none_or(|line| line.key.is_none()));

    let ecdsa = text("id_ecdsa.pub").trim();
    let with_options = format!("command=\"/usr/bin/echo a b\",no-pty {ecdsa}");
    let line = key_line(&with_options).expect("a key");
    assert_eq!(
        line.options.as_deref(),
        Some("command=\"/usr/bin/echo a b\",no-pty")
    );
    assert_eq!(line.comment.as_deref(), Some("ecdsa@example"));
    assert_eq!(
        line.fingerprint,
        "SHA256:XagKGWp0UxWXbK+Vgrs/mk4TE4ZIG2b0amZJKkonYe4"
    );
    assert_eq!(
        line.key.map(|key| (key.algorithm(), key.bits())),
        Some(("EC P-256".to_owned(), Some(256)))
    );
}

#[test]
fn not_a_key() {
    assert_eq!(key_line(""), None);
    assert_eq!(key_line("# ssh-ed25519 AAAA"), None);
    assert_eq!(key_line("ssh-ed25519 not-base64!! x"), None);
    // Base64 of a blob that names another type than the line.
    assert_eq!(
        key_line("ssh-rsa AAAAC3NzaC1lZDI1NTE5AAAAICn7ROXd/OlZni+hgJEiJtYUxuYRvxKMhDuOEoh4RBbz"),
        None
    );
}

#[test]
fn container_gives_the_public_part_only() {
    let block = pem::blocks(fixture("id_ed25519"), 1)
        .into_iter()
        .next()
        .expect("block");
    let body = block.body.expect("body");
    let container = container(&body).expect("openssh-key-v1");
    assert_eq!(container.cipher, None);
    assert_eq!(container.keys.len(), 1);
    let (kind, key, fingerprint) = container.keys.first().expect("one key");
    assert_eq!(kind, "ssh-ed25519");
    assert!(key.is_some());
    assert_eq!(
        fingerprint,
        "SHA256:h4aqM5BLHTKRyRe0Wf93JkqavgnAcEW4PY9jbEUawWI"
    );

    let block = pem::blocks(fixture("id_rsa_enc"), 1)
        .into_iter()
        .next()
        .expect("block");
    let encrypted = super::container(&block.body.expect("body")).expect("openssh-key-v1");
    assert_eq!(encrypted.cipher.as_deref(), Some("aes256-ctr, bcrypt"));
    assert_eq!(
        encrypted.keys.first().map(|(_, _, print)| print.as_str()),
        Some("SHA256:Ham/CDQ4uv2V2CZhJqxQoyZJUGraxXOkNDEcveu0U5A")
    );
}

#[test]
fn damaged_containers_are_none() {
    let block = pem::blocks(fixture("id_ed25519"), 1)
        .into_iter()
        .next()
        .expect("block");
    let body = block.body.expect("body");
    for end in 0..body.len() {
        let _ = container(body.get(..end).unwrap_or_default());
    }
    assert_eq!(container(b"openssh-key-v1\0\0\0\0\x04none"), None);
    assert_eq!(container(b"not-openssh"), None);
}
