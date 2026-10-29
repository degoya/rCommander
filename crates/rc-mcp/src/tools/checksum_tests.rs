//! The checksum tools against a real engine on a temporary folder.

use rc_engine::{Algorithm, Command, Outcome};
use serde_json::json;
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    std::fs::write(fixture.root().join("abc.txt"), b"abc").expect("abc");
    let built = fixture.build().await;
    (built.root, built.context)
}

/// The window computes through the same command; the tool answers the same digest.
#[tokio::test]
async fn files_checksum_answers_what_the_dialog_gets() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let answer = call(
        &client,
        "files_checksum",
        json!({ "paths": ["abc.txt", "missing.txt"], "algorithm": "sha256" }),
    )
    .await;
    assert_eq!(answer["kind"], "checksums", "{answer}");
    assert_eq!(answer["files"][0]["hex"], SHA256_ABC, "{answer}");
    assert_eq!(answer["files"][1]["code"], "not_found", "{answer}");
    let direct = context
        .engine()
        .dispatch(Command::ComputeChecksums(rc_engine::ChecksumRequest {
            panel: None,
            paths: vec!["abc.txt".into()],
            algorithm: Algorithm::Sha256,
        }))
        .await
        .expect("computed");
    let Outcome::Checksums { files, .. } = direct else {
        panic!("checksums expected");
    };
    assert_eq!(
        answer["files"][0]["hex"],
        files[0].hex.clone().expect("hex")
    );
}

#[tokio::test]
async fn create_and_verify_report_a_tampered_file() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let created = call(
        &client,
        "files_checksum_create",
        json!({ "names": ["abc.txt"], "algorithm": "md5" }),
    )
    .await;
    assert_eq!(
        created["report"]["written"],
        json!(["abc.txt.md5"]),
        "{created}"
    );
    std::fs::write(root.path().join("abc.txt"), b"abd").expect("tamper");
    let verified = call(
        &client,
        "files_checksum_verify",
        json!({ "path": "abc.txt.md5" }),
    )
    .await;
    assert_eq!(
        verified["report"]["entries"][0]["status"], "mismatch",
        "{verified}"
    );
    assert_eq!(verified["report"]["summary"]["mismatch"], 1, "{verified}");
}

#[tokio::test]
async fn a_name_that_leaves_the_folder_is_reported_not_read() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    std::fs::create_dir(root.path().join("set")).expect("set");
    std::fs::write(
        root.path().join("set").join("evil.sha256"),
        format!("{SHA256_ABC} *../abc.txt\n"),
    )
    .expect("evil");
    let verified = call(
        &client,
        "files_checksum_verify",
        json!({ "path": "set/evil.sha256" }),
    )
    .await;
    let entry = &verified["report"]["entries"][0];
    assert_eq!(entry["status"], "invalid", "{verified}");
    assert_eq!(entry["code"], "parent_reference", "{verified}");
    assert!(entry.get("actual").is_none(), "{verified}");
}

#[tokio::test]
async fn split_and_combine_round_trip() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let data: Vec<u8> = (0..10_000_u32).map(|n| (n % 253) as u8).collect();
    std::fs::write(root.path().join("disk.iso"), &data).expect("data");
    std::fs::create_dir(root.path().join("back")).expect("back");
    let split = call(
        &client,
        "files_split",
        json!({ "path": "disk.iso", "part_size": 4096 }),
    )
    .await;
    assert_eq!(
        split["report"]["parts"],
        json!(["disk.001", "disk.002", "disk.003"]),
        "{split}"
    );
    assert_eq!(split["report"]["crc_file"], "disk.crc", "{split}");
    let combined = call(
        &client,
        "files_combine",
        json!({ "path": "disk.crc", "target_dir": "back" }),
    )
    .await;
    assert_eq!(combined["report"]["target"], "disk.iso", "{combined}");
    assert_eq!(combined["report"]["crc_checked"], true, "{combined}");
    assert_eq!(
        std::fs::read(root.path().join("back").join("disk.iso")).expect("back"),
        data
    );
    let refused = call(
        &client,
        "files_split",
        json!({ "path": "disk.iso", "part_size": 0 }),
    )
    .await;
    assert_eq!(refused["code"], "split_part_size_invalid", "{refused}");
}

/// A digest, a checksum file's lines and split parts come from a file's contents: every tool
/// refuses a key or credential file, and a folder whose files all are.
#[tokio::test]
async fn sensitive_files_are_refused_by_every_checksum_tool() {
    let (root, context) = setup().await;
    std::fs::create_dir(root.path().join(".ssh")).expect("mkdir");
    std::fs::write(root.path().join(".ssh/id_ed25519"), b"key").expect("key");
    std::fs::write(root.path().join("server.pem"), b"key").expect("pem");
    let client = connect(&context).await;
    for (tool, arguments) in [
        (
            "files_checksum",
            json!({ "paths": ["abc.txt", "server.pem"], "algorithm": "sha256" }),
        ),
        (
            "files_checksum_create",
            json!({ "names": [".ssh"], "algorithm": "sha256" }),
        ),
        (
            "files_checksum_create",
            json!({ "dir": ".ssh", "names": ["id_ed25519"], "algorithm": "md5" }),
        ),
        (
            "files_checksum_verify",
            json!({ "path": ".ssh/id_ed25519" }),
        ),
        (
            "files_split",
            json!({ "path": "server.pem", "part_size": 1 }),
        ),
        ("files_combine", json!({ "path": ".ssh/id_ed25519.001" })),
    ] {
        let answer = call(&client, tool, arguments.clone()).await;
        assert_eq!(
            answer["code"], "sensitive_path",
            "{tool} {arguments}: {answer}"
        );
    }
    assert!(!root.path().join("server.001").exists());
    assert!(!root.path().join("checksums.sha256").exists());
}

/// The folder probe knows folder rules only: a credential file deep inside an ordinary folder,
/// or named in a checksum file's line, is still never hashed - the engine judges every file.
#[tokio::test]
async fn a_sensitive_file_inside_a_folder_or_a_checksum_file_is_never_hashed() {
    let (root, context) = setup().await;
    std::fs::create_dir_all(root.path().join("project/conf")).expect("mkdir");
    std::fs::write(root.path().join("project/conf/.env"), b"abc").expect("env");
    std::fs::write(root.path().join("server.pem"), b"abc").expect("pem");
    std::fs::write(
        root.path().join("list.sha256"),
        format!("{SHA256_ABC} *abc.txt\n{SHA256_ABC} *server.pem\n"),
    )
    .expect("list");
    let client = connect(&context).await;
    let created = call(
        &client,
        "files_checksum_create",
        json!({ "names": ["project"], "algorithm": "sha256" }),
    )
    .await;
    assert_eq!(created["code"], "sensitive_path", "{created}");
    assert!(!root.path().join("project.sha256").exists());
    let verified = call(
        &client,
        "files_checksum_verify",
        json!({ "path": "list.sha256" }),
    )
    .await;
    assert_eq!(
        verified["report"]["entries"][0]["status"], "ok",
        "{verified}"
    );
    let guarded = &verified["report"]["entries"][1];
    assert_eq!(guarded["status"], "unreadable", "{verified}");
    assert_eq!(guarded["code"], "sensitive_path", "{verified}");
    assert!(guarded.get("actual").is_none(), "{verified}");
}
