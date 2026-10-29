use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;

const ED25519: &str = include_str!("../tests/fixtures/client_key_ed25519");
const ED25519_PUB: &str = include_str!("../tests/fixtures/client_key_ed25519.pub");
/// An encrypted PKCS#1 RSA key (`ssh-keygen -m PEM`), passphrase [`RSA_PASSPHRASE`].
const RSA_PEM: &str = include_str!("../tests/fixtures/client_key_rsapem");
const RSA_PEM_PUB: &str = include_str!("../tests/fixtures/client_key_rsapem.pub");
const RSA_PASSPHRASE: &str = "fixture-passphrase";
/// ssh-key's own examples: an unencrypted PPK v3 ed25519 key and an encrypted PPK v2 RSA key
/// with the passphrase `123`.
const PPK3: &str = include_str!("../tests/fixtures/client_key_ed25519ppk");
const PPK2_ENCRYPTED: &str = include_str!("../tests/fixtures/client_key_rsappk2");

/// The `<type> <base64>` part of an `authorized_keys` line.
fn public_part(line: &str) -> String {
    line.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn an_ed25519_key_is_described_by_its_public_half() {
    let info = inspect(ED25519, None).expect("an ed25519 key");
    assert_eq!(info.format, KeyFormat::OpenSsh);
    assert!(!info.encrypted);
    assert_eq!(info.algorithm, "ssh-ed25519");
    assert_eq!(
        info.fingerprint,
        "SHA256:VqHchmOpCuMbpd8yWqFHO6llI9815J85wGd1QUaKRig"
    );
    assert_eq!(info.comment.as_deref(), Some("fixture@rcommander"));
    assert_eq!(info.public_key, public_part(ED25519_PUB));
    // A passphrase for a key that has none is not needed and does no harm.
    assert_eq!(inspect(ED25519, Some("unused")).expect("still opens"), info);
}

#[test]
fn an_encrypted_rsa_key_needs_its_passphrase() {
    assert_eq!(check_format(RSA_PEM), Ok(KeyFormat::Pem));
    assert_eq!(
        inspect(RSA_PEM, None),
        Err(PrivateKeyError::PassphraseNeeded)
    );
    assert_eq!(
        inspect(RSA_PEM, Some("not the passphrase")),
        Err(PrivateKeyError::PassphraseWrong)
    );
    let info = inspect(RSA_PEM, Some(RSA_PASSPHRASE)).expect("opens");
    assert!(info.encrypted);
    assert_eq!(info.algorithm, "ssh-rsa");
    assert_eq!(
        info.fingerprint,
        "SHA256:6KajqcW1Orb7TSswcvfa2oFhUNaLC46DsrFV+00r028"
    );
    assert_eq!(info.public_key, public_part(RSA_PEM_PUB));
    let key = decode(RSA_PEM, Some(RSA_PASSPHRASE)).expect("decodes");
    assert_eq!(
        key.public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string(),
        info.fingerprint
    );
}

#[test]
fn putty_keys_of_version_2_and_3_are_taken() {
    let v3 = inspect(PPK3, None).expect("ppk v3");
    assert_eq!(v3.format, KeyFormat::Ppk);
    assert!(!v3.encrypted);
    assert_eq!(
        v3.fingerprint,
        "SHA256:UCUiLr7Pjs9wFFJMDByLgc3NrtdU344OgUM45wZPcIQ"
    );
    assert_eq!(v3.comment.as_deref(), Some("user@example.com"));

    assert_eq!(
        inspect(PPK2_ENCRYPTED, None),
        Err(PrivateKeyError::PassphraseNeeded)
    );
    assert_eq!(
        inspect(PPK2_ENCRYPTED, Some("wrong")),
        Err(PrivateKeyError::PassphraseWrong)
    );
    let v2 = inspect(PPK2_ENCRYPTED, Some("123")).expect("ppk v2");
    assert!(v2.encrypted);
    assert_eq!(v2.algorithm, "ssh-rsa");
    assert_eq!(
        v2.fingerprint,
        "SHA256:Fmxts/GcV77PakFnf1Ueki5mpU4ZjUQWGRjZGAo3n/I"
    );
}

#[test]
fn the_armour_is_checked_before_any_parser_sees_the_text() {
    for label in ["EC PRIVATE KEY", "PRIVATE KEY", "ENCRYPTED PRIVATE KEY"] {
        let key = format!("-----BEGIN {label}-----\nMHcCAQEEIA==\n-----END {label}-----");
        assert!(check_format(&key).is_ok(), "{label}");
    }
    let long = format!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----",
        "A".repeat(MAX_KEY_BYTES)
    );
    for text in [
        "",
        "hunter2",
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB user@host",
        "PuTTY-User-Key-File-1: ssh-rsa\n",
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA\n-----END PUBLIC KEY-----",
        "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----",
        "-----BEGIN DSA PRIVATE KEY-----\nMIIB\n-----END DSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----\n-----END OPENSSH PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbn\n-----END RSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nnot base64!\n-----END OPENSSH PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbn",
        long.as_str(),
    ] {
        assert_eq!(
            check_format(text),
            Err(PrivateKeyError::Unknown),
            "{text:.60}"
        );
    }
}

#[test]
fn well_armoured_garbage_does_not_decode() {
    let fake = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n\
                -----END OPENSSH PRIVATE KEY-----";
    assert_eq!(check_format(fake), Ok(KeyFormat::OpenSsh));
    assert_eq!(inspect(fake, None), Err(PrivateKeyError::Unknown));
}

#[test]
fn the_description_carries_no_private_material() {
    let info = inspect(ED25519, None).expect("opens");
    let shown = format!("{info:?}");
    for line in ED25519.lines().filter(|line| !line.starts_with("-----")) {
        assert!(!shown.contains(line.trim()), "{shown}");
    }
}

/// An OpenSSH key file encrypted with bcrypt-pbkdf over `rounds` rounds; its secret part is
/// noise.
fn openssh_with_rounds(rounds: u32) -> String {
    let string = |bytes: &[u8]| {
        let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(bytes);
        out
    };
    let public = STANDARD
        .decode(ED25519_PUB.split_whitespace().nth(1).expect("key"))
        .expect("base64");
    let mut options = string(&[7; 16]);
    options.extend(rounds.to_be_bytes());
    let mut blob = b"openssh-key-v1\0".to_vec();
    blob.extend(string(b"aes256-ctr"));
    blob.extend(string(b"bcrypt"));
    blob.extend(string(&options));
    blob.extend(1_u32.to_be_bytes());
    blob.extend(string(&public));
    blob.extend(string(&[0; 64]));
    let body = STANDARD.encode(blob);
    let lines: Vec<&str> = body
        .as_bytes()
        .chunks(70)
        .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ASCII"))
        .collect();
    format!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----\n",
        lines.join("\n")
    )
}

/// The PPK v3 fixture as an encrypted key whose argon2 asks for `memory` KiB over `passes`.
fn ppk3_with_argon2(memory: u64, passes: u64) -> String {
    PPK3.replace("Encryption: none", "Encryption: aes256-cbc")
        .replace(
            "Private-Lines:",
            &format!(
                "Key-Derivation: Argon2id\nArgon2-Memory: {memory}\nArgon2-Passes: {passes}\n\
                 Argon2-Parallelism: 1\nArgon2-Salt: 0011223344556677\nPrivate-Lines:"
            ),
        )
}

/// RC-080-12 (X7): a key file that asks its key derivation for more than the bounds is refused
/// before any derivation runs - never a hang or an exhausted memory; one within them is opened
/// (and here, as its secret part is noise, refused for the passphrase).
#[test]
fn a_key_derivation_over_its_bounds_is_refused_before_it_runs() {
    let hostile = [
        openssh_with_rounds(u32::MAX),
        openssh_with_rounds(MAX_BCRYPT_ROUNDS + 1),
        ppk3_with_argon2(64 * 1024 * 1024 * 1024, 1),
        ppk3_with_argon2(8192, 1_000_000),
    ];
    for text in &hostile {
        assert!(check_format(text).is_ok(), "{text}");
        assert_eq!(
            inspect(text, Some("x")),
            Err(PrivateKeyError::Unknown),
            "{text}"
        );
    }
    for text in [openssh_with_rounds(2), ppk3_with_argon2(8192, 1)] {
        assert_eq!(
            inspect(&text, Some("x")),
            Err(PrivateKeyError::PassphraseWrong),
            "{text}"
        );
    }
}

/// RC-080-43: every armour the reader takes in, and PuTTY's header, is one the marker table
/// (`key_markers`) recognises, so the MCP mask and the import guard see what the reader reads.
#[test]
fn every_format_the_reader_takes_carries_a_marker() {
    for (label, _) in LABELS {
        let armour = format!("-----BEGIN {label}-----\n");
        assert!(crate::key_markers::holds_private_key(&armour), "{label}");
    }
    assert!(crate::key_markers::holds_private_key(PPK_PREFIX));
}
