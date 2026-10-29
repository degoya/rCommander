use super::*;
use crate::{MemoryFetcher, sign::hex_sha256};

const URL: &str = "https://github.com/degoya/rCommander/releases/download/v1.8.0/rcommander-1.8.0-windows-x64.zip";

fn artifact_for(body: &[u8]) -> Artifact {
    Artifact {
        platform: "windows".to_owned(),
        arch: "x86_64".to_owned(),
        kind: "zip".to_owned(),
        url: URL.to_owned(),
        sha256: hex_sha256(body),
        size: body.len() as u64,
    }
}

fn body() -> Vec<u8> {
    (0..200u8).collect()
}

fn files(directory: &Path) -> usize {
    std::fs::read_dir(directory).expect("list").count()
}

async fn fetch(
    fetcher: &MemoryFetcher,
    artifact: &Artifact,
    directory: &Path,
) -> Result<PathBuf, UpdateError> {
    download_verified(fetcher, artifact, directory, &Progress::default()).await
}

#[tokio::test]
async fn a_matching_download_is_kept_under_its_name() {
    let directory = tempfile::tempdir().expect("tempdir");
    let fetcher = MemoryFetcher::new();
    fetcher.serve(URL, body());
    let path = fetch(&fetcher, &artifact_for(&body()), directory.path())
        .await
        .expect("download");
    assert_eq!(
        path,
        directory.path().join("rcommander-1.8.0-windows-x64.zip")
    );
    assert_eq!(std::fs::read(&path).expect("read"), body());
}

/// A file already there is hashed again instead of loaded again; changed since, it is not the
/// signed file any more.
#[tokio::test]
async fn a_kept_download_is_found_again_only_while_it_is_the_signed_file() {
    let directory = tempfile::tempdir().expect("tempdir");
    let fetcher = MemoryFetcher::new();
    fetcher.serve(URL, body());
    let artifact = artifact_for(&body());
    assert_eq!(verified_file(&artifact, directory.path()).await, None);
    let progress = Progress::default();
    let path = download_verified(&fetcher, &artifact, directory.path(), &progress)
        .await
        .expect("download");
    assert_eq!(progress.received(), artifact.size);
    assert_eq!(
        verified_file(&artifact, directory.path()).await,
        Some(path.clone())
    );
    let mut changed = body();
    changed[3] ^= 0xff;
    std::fs::write(&path, changed).expect("change");
    assert_eq!(verified_file(&artifact, directory.path()).await, None);
}

#[tokio::test]
async fn a_download_with_the_wrong_hash_leaves_no_file() {
    let directory = tempfile::tempdir().expect("tempdir");
    let fetcher = MemoryFetcher::new();
    let mut forged = body();
    forged[17] ^= 0xff;
    fetcher.serve(URL, forged);
    let error = fetch(&fetcher, &artifact_for(&body()), directory.path())
        .await
        .expect_err("bad hash");
    assert_eq!(error.code(), "update.digest_mismatch");
    assert_eq!(files(directory.path()), 0);
}

#[tokio::test]
async fn a_download_with_more_or_fewer_bytes_leaves_no_file() {
    for served in [
        {
            let mut longer = body();
            longer.push(0);
            longer
        },
        body()[..150].to_vec(),
    ] {
        let directory = tempfile::tempdir().expect("tempdir");
        let fetcher = MemoryFetcher::new();
        fetcher.serve(URL, served);
        let error = fetch(&fetcher, &artifact_for(&body()), directory.path())
            .await
            .expect_err("wrong size");
        assert_eq!(error.code(), "update.size_mismatch");
        assert_eq!(files(directory.path()), 0);
    }
}

#[tokio::test]
async fn an_unreachable_artifact_is_a_download_failure() {
    let directory = tempfile::tempdir().expect("tempdir");
    let error = fetch(
        &MemoryFetcher::new(),
        &artifact_for(&body()),
        directory.path(),
    )
    .await
    .expect_err("404");
    assert_eq!(error.code(), "update.download_failed");
}

#[tokio::test]
async fn a_cancelled_download_leaves_no_file() {
    let directory = tempfile::tempdir().expect("tempdir");
    let fetcher = MemoryFetcher::new();
    fetcher.serve(URL, body());
    let progress = Progress::default();
    progress.cancel();
    let error = download_verified(
        &fetcher,
        &artifact_for(&body()),
        directory.path(),
        &progress,
    )
    .await
    .expect_err("cancelled");
    assert_eq!(error.code(), "update.cancelled");
    assert_eq!(files(directory.path()), 0);
}

#[test]
fn twice_the_artifact_must_be_free() {
    let artifact = artifact_for(&body());
    assert!(check_space(&artifact, Some(400)).is_ok());
    let error = check_space(&artifact, Some(399)).expect_err("no space");
    assert_eq!(error.code(), "update.no_space");
    // A volume that does not say is not refused for it.
    assert!(check_space(&artifact, None).is_ok());
}

#[test]
fn the_file_name_cannot_leave_the_directory() {
    let url = url::Url::parse("https://example.test/a/..%2F..%2Fetc%2Fpasswd").expect("url");
    assert!(!file_name(&url).contains('/'));
    let url = url::Url::parse("https://example.test/").expect("url");
    assert_eq!(file_name(&url), "rcommander-update.bin");
}
