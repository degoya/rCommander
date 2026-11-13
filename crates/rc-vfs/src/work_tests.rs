use std::path::Path;

use super::blocking;
use crate::VfsError;

#[tokio::test]
async fn the_work_s_answer_comes_back() {
    let path = Path::new("/data/a.txt");
    assert_eq!(blocking("test.work", path, || Ok(7_u8)).await.ok(), Some(7));
    let refused = blocking("test.work", path, || -> crate::Result<u8> {
        Err(VfsError::NotFound {
            path: path.to_path_buf(),
        })
    })
    .await;
    assert!(
        matches!(refused, Err(VfsError::NotFound { .. })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn work_that_panicked_is_an_io_error_on_the_path() {
    let path = Path::new("/data/a.txt");
    let died = blocking("test.work", path, || -> crate::Result<u8> {
        panic!("the test's own panic")
    })
    .await;
    assert!(
        matches!(&died, Err(VfsError::Io { path: at, .. }) if at == path),
        "{died:?}"
    );
}
