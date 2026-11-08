use rc_db::{DB_FILE, Db};
use rc_update::portable_mode::{DATA_DIR, MARKER, MARKER_TEXT};

use super::make_portable_at;
use crate::{Command, CommandClass, test_support};

/// The person's database in its own folder, and a program folder to make portable.
async fn scene() -> (tempfile::TempDir, Db, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open(root.path().join("profile").join(DB_FILE))
        .await
        .expect("db");
    db.set("view.hidden", &true).await.expect("setting");
    let program = root.path().join("stick");
    std::fs::create_dir_all(&program).expect("program folder");
    (root, db, program)
}

#[tokio::test]
async fn make_portable_never_overwrites_a_database() {
    let (_root, db, program) = scene().await;
    let data = program.join(DATA_DIR);
    std::fs::create_dir_all(&data).expect("data");
    std::fs::write(data.join(DB_FILE), b"a database already there").expect("write");
    let error = make_portable_at(&db, &program, &data, None)
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "update.portable_database_exists");
    assert_eq!(
        std::fs::read(data.join(DB_FILE)).expect("read"),
        b"a database already there"
    );
    assert!(!program.join(MARKER).exists());
}

#[tokio::test]
async fn make_portable_writes_the_marker_last() {
    // A whole run: the checked copy, the plugins, then the marker.
    let (root, db, program) = scene().await;
    let plugins = root.path().join("profile").join("plugins");
    std::fs::create_dir_all(plugins.join("viewer-x")).expect("plugins");
    std::fs::write(plugins.join("viewer-x").join("plugin.wasm"), b"wasm").expect("plugin");
    std::fs::write(plugins.join("state.json"), b"{}").expect("state");
    let data = program.join(DATA_DIR);
    let report = make_portable_at(&db, &program, &data, Some(&plugins))
        .await
        .expect("made portable");
    assert_eq!(report.path, data.join(DB_FILE));
    assert!(rc_db::is_rcommander_database(&report.path).await);
    assert_eq!(
        std::fs::read(data.join("plugins").join("viewer-x").join("plugin.wasm")).expect("copied"),
        b"wasm"
    );
    assert_eq!(
        std::fs::read_to_string(program.join(MARKER)).expect("marker"),
        MARKER_TEXT
    );
    let copy = Db::open(&report.path).await.expect("the copy opens");
    assert_eq!(
        copy.get::<bool>("view.hidden").await.expect("read"),
        Some(true)
    );

    // A step before the marker fails: no marker, and nothing half done stays.
    let (_root, db, program) = scene().await;
    let data = program.join(DATA_DIR);
    std::fs::write(&data, b"a file where the folder would be").expect("file");
    make_portable_at(&db, &program, &data, None)
        .await
        .expect_err("no folder");
    assert!(!program.join(MARKER).exists());
    assert!(data.is_file(), "what was there stays");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let (root, db, program) = scene().await;
        let plugins = root.path().join("profile").join("plugins");
        std::fs::create_dir_all(&plugins).expect("plugins");
        let unreadable = plugins.join("secret.bin");
        std::fs::write(&unreadable, b"x").expect("write");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
            .expect("chmod");
        if std::fs::read(&unreadable).is_ok() {
            eprintln!("permissions do not hold for this user; the plugin step cannot fail");
            return;
        }
        let data = program.join(DATA_DIR);
        make_portable_at(&db, &program, &data, Some(&plugins))
            .await
            .expect_err("the plugins cannot be copied");
        assert!(!program.join(MARKER).exists(), "the marker comes last");
        assert!(!data.exists(), "the copy went again");
    }
}

#[tokio::test]
async fn make_portable_is_gui_only() {
    let spec = crate::spec("app.make_portable").expect("registered");
    assert_eq!(spec.class, CommandClass::GuiOnly);
    let data = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(data.path()).await;
    let refused = engine
        .dispatch_mcp(Command::MakePortable {})
        .await
        .expect_err("gui only");
    assert_eq!(refused.code(), "gui_only");
}
