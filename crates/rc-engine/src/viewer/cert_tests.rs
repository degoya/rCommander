use std::fs;

use super::*;
use crate::viewer::test_support::local;

const LEAF: &[u8] = include_bytes!("../../../rc-cert/tests/fixtures/leaf.crt");
const KEY: &[u8] = include_bytes!("../../../rc-cert/tests/fixtures/leaf-rsa.key");

async fn report_of(name: &str, bytes: &[u8]) -> Option<Report> {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(name);
    fs::write(&path, bytes).expect("write");
    let file = FileWindow::open(local(), &path).await.expect("open");
    let head: Vec<u8> = bytes.iter().take(4).copied().collect();
    read_report(&file, &path, &head, None).await
}

#[test]
fn candidates_by_extension_name_or_armour() {
    assert!(is_candidate(Path::new("/srv/tls/fullchain.PEM"), b""));
    assert!(is_candidate(Path::new("server.key"), b"MII"));
    assert!(is_candidate(Path::new("/home/me/.ssh/id_ed25519"), b"----"));
    assert!(is_candidate(
        Path::new("/home/me/.ssh/authorized_keys"),
        b"ssh-"
    ));
    assert!(is_candidate(Path::new("notes.txt"), b"----"));
    assert!(!is_candidate(Path::new("notes.txt"), b"hell"));
    assert!(!is_candidate(Path::new("photo.jpg"), b"\xff\xd8\xff\xe0"));
}

#[tokio::test]
async fn a_certificate_file_is_reported() {
    let report = report_of("leaf.crt", LEAF).await.expect("a report");
    assert_eq!(report.sections.len(), 1);
    assert!(!report.checks.is_empty());
}

#[tokio::test]
async fn a_key_and_a_certificate_under_any_name_by_their_armour() {
    let mut both = LEAF.to_vec();
    both.extend_from_slice(KEY);
    let report = report_of("bundle.txt", &both).await.expect("a report");
    assert_eq!(report.sections.len(), 2);
}

#[tokio::test]
async fn other_files_are_no_report() {
    assert!(
        report_of("readme.pem", b"not a certificate\n")
            .await
            .is_none()
    );
    assert!(report_of("notes.txt", b"hello\n").await.is_none());
    assert!(report_of("empty.pem", b"").await.is_none());
}
