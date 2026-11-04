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

/// A link below the start keeps its target's mode: `key -> ../outside/secret` with a recursive
/// 0755 leaves the secret, outside the tree, at 0600 (RC-020-06).
#[cfg(unix)]
#[tokio::test]
async fn a_recursive_mode_does_not_reach_through_a_link() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let root = tempfile::tempdir().expect("temp");
    let outside = root.path().join("outside");
    let tree = root.path().join("tree");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::create_dir_all(&tree).expect("tree");
    let secret = outside.join("secret");
    std::fs::write(&secret, b"key").expect("secret");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).expect("0600");
    std::fs::write(tree.join("plain"), b"x").expect("plain");
    symlink(&secret, tree.join("key")).expect("link");

    let fs = rc_vfs::LocalFs::new();
    let mut report = AttributeReport::default();
    // 0755, not 0644: the start folder takes the mode too, and without x it could not be
    // walked - which only root, ignoring permissions, would not notice.
    let change = PropertyChange {
        mode: Some(0o755),
        ..PropertyChange::default()
    };
    set_properties_tree(&fs, &tree, change, true, ApplyTo::default(), &mut report)
        .await
        .expect("set");

    let mode = |path: &Path| std::fs::metadata(path).expect("stat").permissions().mode() & 0o7777;
    assert_eq!(mode(&secret), 0o600, "the target outside the tree");
    assert_eq!(mode(&tree.join("plain")), 0o755);
    assert_eq!(report.failed, 0, "{report:?}");
}
