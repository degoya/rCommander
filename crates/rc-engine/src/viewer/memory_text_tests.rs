use super::*;

#[tokio::test]
async fn the_lister_reads_the_text_from_memory() {
    let file = open_text("report.txt", "Checks\nline two\n".to_owned())
        .await
        .expect("opened");
    assert_eq!(file.size(), 16);
    assert_eq!(file.read_at(7, 4).await.expect("read"), b"line");
    assert_eq!(file.read_uncached(0, 6).await.expect("read"), b"Checks");
    assert!(!file.path().exists(), "nothing is on a disk");
}

#[tokio::test]
async fn nothing_else_is_there_and_nothing_can_change() {
    let backend = MemoryText {
        path: PathBuf::from("report.txt"),
        bytes: Arc::from(b"x".to_vec()),
        made: SystemTime::now(),
    };
    let other = Path::new("other.txt");
    assert!(matches!(
        backend.stat(other).await,
        Err(VfsError::NotFound { .. })
    ));
    assert!(backend.list(Path::new("/")).await.is_err());
    let path = Path::new("report.txt");
    assert!(backend.open_write(path, WriteMode::Truncate).await.is_err());
    assert!(backend.remove_file(path).await.is_err());
    assert!(backend.rename(path, other).await.is_err());
    assert_eq!(
        backend.stat(path).await.map(|entry| entry.size).ok(),
        Some(1)
    );
}
