use std::path::Path;

use secrecy::SecretString;

use super::{super::test_support::*, ArchivePassword};
use crate::Vfs;

fn password(text: &str) -> ArchivePassword {
    ArchivePassword::new(SecretString::from(text))
}

/// Acceptance (RC-030-03): an encrypted zip and 7z read with the right password.
#[tokio::test]
async fn the_right_password_reads_encrypted_entries() {
    let cases: &[(&str, &[u8])] = &[
        ("encrypted-zipcrypto.zip", b"zipcrypto secret\n"),
        ("encrypted-aes.zip", b"aes secret\n"),
        ("encrypted.7z", b"7z secret\n"),
        ("encrypted-header.7z", b"7z secret\n"),
    ];
    for (name, content) in cases {
        let fs = open(name).await.with_password(password("secret"));
        assert_eq!(read(&fs, "/secret.txt").await, *content, "{name}");
    }
}

/// Acceptance (RC-030-03): a wrong password is told from a missing one.
#[tokio::test]
async fn a_wrong_password_is_password_wrong() {
    for name in ["encrypted-aes.zip", "encrypted.7z", "encrypted-header.7z"] {
        let fs = open(name).await.with_password(password("not the secret"));
        let read = match fs.list(Path::new("/")).await {
            // A 7z with an encrypted header already fails to list.
            Err(error) => Err(error),
            Ok(_) => fs.open_read(Path::new("/secret.txt")).await.map(|_| ()),
        };
        assert_eq!(code(read), "password_wrong", "{name}");
    }
}

#[tokio::test]
async fn without_a_password_it_is_still_required() {
    let fs = open("encrypted.7z").await;
    assert_eq!(
        code(fs.open_read(Path::new("/secret.txt")).await),
        "password_required"
    );
}

#[test]
fn debug_never_shows_the_password() {
    let shown = format!("{:?}", password("canary-7f3a"));
    assert!(!shown.contains("canary"), "{shown}");
}
