use std::path::Path;

use rc_vfs::{MemoryFs, PropertyChange, Vfs};
use tokio::io::AsyncWriteExt;

use super::{ApplyTo, AttributeReport, set_properties_tree};

async fn file(fs: &MemoryFs, path: &str) {
    let mut stream = fs
        .open_write(Path::new(path), rc_vfs::WriteMode::CreateNew)
        .await
        .expect("open");
    stream.write_all(b"x").await.expect("write");
    stream.shutdown().await.expect("shutdown");
}

async fn tree() -> MemoryFs {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/t/sub"), true).await.expect("mkdir");
    file(&fs, "/t/a").await;
    file(&fs, "/t/sub/b").await;
    fs
}

fn readonly() -> PropertyChange {
    PropertyChange {
        readonly: Some(true),
        ..PropertyChange::default()
    }
}

async fn is_readonly(fs: &MemoryFs, path: &str) -> bool {
    fs.stat(Path::new(path))
        .await
        .expect("stat")
        .attributes
        .readonly
}

#[tokio::test]
async fn without_recursion_only_the_start_changes() {
    let fs = tree().await;
    let mut report = AttributeReport::default();
    set_properties_tree(
        &fs,
        Path::new("/t"),
        readonly(),
        false,
        ApplyTo::default(),
        &mut report,
    )
    .await
    .expect("set");
    assert_eq!(report.changed, 1);
    assert!(is_readonly(&fs, "/t").await);
    assert!(!is_readonly(&fs, "/t/a").await);
}

#[tokio::test]
async fn recursion_reaches_every_level() {
    let fs = tree().await;
    let mut report = AttributeReport::default();
    set_properties_tree(
        &fs,
        Path::new("/t"),
        readonly(),
        true,
        ApplyTo::default(),
        &mut report,
    )
    .await
    .expect("set");
    assert_eq!(report.changed, 4);
    for path in ["/t", "/t/a", "/t/sub", "/t/sub/b"] {
        assert!(is_readonly(&fs, path).await, "{path}");
    }
}

#[tokio::test]
async fn files_only_leaves_the_folders_below() {
    let fs = tree().await;
    let mut report = AttributeReport::default();
    let files = ApplyTo {
        files: true,
        dirs: false,
    };
    set_properties_tree(&fs, Path::new("/t"), readonly(), true, files, &mut report)
        .await
        .expect("set");
    assert!(is_readonly(&fs, "/t/sub/b").await);
    assert!(!is_readonly(&fs, "/t/sub").await);
    assert!(
        is_readonly(&fs, "/t").await,
        "the chosen entry always changes"
    );
}

#[tokio::test]
async fn an_unsupported_change_fails_at_the_start() {
    let fs = tree().await;
    let mut report = AttributeReport::default();
    let hidden = PropertyChange {
        hidden: Some(true),
        ..PropertyChange::default()
    };
    let result = set_properties_tree(
        &fs,
        Path::new("/t"),
        hidden,
        true,
        ApplyTo::default(),
        &mut report,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(report.changed, 0);
}
