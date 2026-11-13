use super::read_within;
use crate::{LocalFs, VfsError};

#[tokio::test]
async fn a_file_within_the_limit_is_read_whole_and_a_larger_one_is_none() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("four.bin");
    std::fs::write(&path, b"abcd").expect("write");
    let read = |limit| read_within(&LocalFs, &path, limit);
    assert_eq!(read(4).await.expect("read"), Some(b"abcd".to_vec()));
    assert_eq!(read(u64::MAX).await.expect("read"), Some(b"abcd".to_vec()));
    assert_eq!(read(3).await.expect("read"), None);
    assert_eq!(read(0).await.expect("read"), None);
}

#[tokio::test]
async fn a_missing_file_is_the_backend_s_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("none.bin");
    let read = read_within(&LocalFs, &missing, 8).await;
    assert!(matches!(read, Err(VfsError::NotFound { .. })), "{read:?}");
}
