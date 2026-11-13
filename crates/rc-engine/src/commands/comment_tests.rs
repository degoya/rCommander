use std::fs;

use crate::test_support;
use crate::{Command, DESCRIPTION_FILE, Engine, Outcome};

async fn engine() -> (tempfile::TempDir, Engine) {
    test_support::engine(&[("notes.txt", "x"), ("my file.txt", "y")]).await
}

fn set(path: &str, comment: &str) -> Command {
    Command::SetComment {
        panel: None,
        path: Some(path.into()),
        comment: Some(comment.into()),
    }
}

#[tokio::test]
async fn a_comment_is_written_read_and_removed() {
    let (root, engine) = engine().await;
    engine
        .dispatch(set("notes.txt", "shopping list"))
        .await
        .expect("set");
    engine
        .dispatch(set("my file.txt", "second"))
        .await
        .expect("set");
    let file = root.path().join(DESCRIPTION_FILE);
    assert_eq!(
        fs::read_to_string(&file).expect("read"),
        "notes.txt shopping list\r\n\"my file.txt\" second\r\n"
    );
    let Outcome::Properties(report) = engine
        .dispatch(Command::GetProperties {
            panel: None,
            paths: vec!["notes.txt".into()],
            count: false,
        })
        .await
        .expect("properties")
    else {
        panic!("properties expected");
    };
    assert_eq!(report.items[0].comment.as_deref(), Some("shopping list"));

    engine
        .dispatch(set("notes.txt", ""))
        .await
        .expect("removed");
    engine
        .dispatch(set("my file.txt", ""))
        .await
        .expect("removed");
    assert!(!file.exists(), "an empty descript.ion goes");
}

#[tokio::test]
async fn a_missing_entry_or_comment_is_refused() {
    let (_root, engine) = engine().await;
    let missing = engine
        .dispatch(set("nothing.txt", "x"))
        .await
        .expect_err("refused");
    assert_eq!(missing.code(), "not_found");
    let asked = engine
        .dispatch(Command::SetComment {
            panel: None,
            path: Some("notes.txt".into()),
            comment: None,
        })
        .await
        .expect_err("refused");
    assert_eq!(asked.code(), "property_invalid");
    let long = "x".repeat(crate::MAX_COMMENT + 1);
    let too_long = engine
        .dispatch(set("notes.txt", &long))
        .await
        .expect_err("refused");
    assert_eq!(too_long.code(), "property_invalid");
}

/// Two comments in OEM 850 ("gr\u{f6}\u{df}e", "\u{e4}\u{fc}"), which is no valid UTF-8.
const OEM: &[u8] = b"alt.txt gr\x94\xe1e\r\nnotes.txt old\r\n\"my file.txt\" \x84\x81\r\n";

fn names(root: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_code_page_file_changes_in_one_comment_only() {
    let (root, engine) = engine().await;
    let file = root.path().join(DESCRIPTION_FILE);
    fs::write(&file, OEM).expect("write");
    engine.dispatch(set("notes.txt", "new")).await.expect("set");
    assert_eq!(
        fs::read(&file).expect("read"),
        b"alt.txt gr\x94\xe1e\r\nnotes.txt new\r\n\"my file.txt\" \x84\x81\r\n"
    );
    assert_eq!(
        names(root.path()),
        [DESCRIPTION_FILE, "my file.txt", "notes.txt"],
        "no temporary file is left"
    );
}

#[tokio::test]
async fn a_comment_the_file_cannot_hold_is_refused_with_its_own_code() {
    let (root, engine) = engine().await;
    let file = root.path().join(DESCRIPTION_FILE);
    fs::write(&file, OEM).expect("write");
    let refused = engine
        .dispatch(set("notes.txt", "\u{416}"))
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "comment_not_encodable");
    assert_eq!(fs::read(&file).expect("read"), OEM, "nothing was written");
    assert_eq!(
        names(root.path()),
        [DESCRIPTION_FILE, "my file.txt", "notes.txt"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn comments_set_at_once_are_all_kept() {
    let files: Vec<String> = (0..16).map(|at| format!("f{at}.txt")).collect();
    let entries: Vec<(&str, &str)> = files.iter().map(|name| (name.as_str(), "x")).collect();
    let (root, engine) = test_support::engine(&entries).await;
    let mut tasks = tokio::task::JoinSet::new();
    for name in &files {
        let engine = engine.clone();
        let command = set(name, &format!("about {name}"));
        tasks.spawn(async move { engine.dispatch(command).await.map(|_| ()) });
    }
    while let Some(done) = tasks.join_next().await {
        done.expect("joined").expect("set");
    }
    let text = fs::read_to_string(root.path().join(DESCRIPTION_FILE)).expect("read");
    for name in &files {
        assert!(
            text.contains(&format!("{name} about {name}\r\n")),
            "{name} is missing in {text:?}"
        );
    }
    assert_eq!(text.lines().count(), files.len());
}

/// A folder that takes no new file: the temporary file cannot be made, so the write fails
/// before anything of the old file is touched. Written in place, the same folder would have let
/// the old file be emptied first.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_write_leaves_the_old_file() {
    use std::os::unix::fs::PermissionsExt as _;

    let (root, engine) = engine().await;
    let file = root.path().join(DESCRIPTION_FILE);
    fs::write(&file, OEM).expect("write");
    let closed = fs::Permissions::from_mode(0o555);
    let open = fs::Permissions::from_mode(0o755);
    fs::set_permissions(root.path(), closed).expect("chmod");
    // Root writes into any folder: nothing to show there.
    let probe = root.path().join("probe");
    if fs::write(&probe, "x").is_ok() {
        fs::set_permissions(root.path(), open).expect("chmod");
        return;
    }
    let failed = engine.dispatch(set("notes.txt", "new")).await;
    fs::set_permissions(root.path(), open).expect("chmod");
    assert_eq!(failed.expect_err("refused").code(), "permission_denied");
    assert_eq!(fs::read(&file).expect("read"), OEM);
    assert_eq!(
        names(root.path()),
        [DESCRIPTION_FILE, "my file.txt", "notes.txt"]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn the_new_file_keeps_the_old_ones_permission_bits() {
    use std::os::unix::fs::PermissionsExt as _;

    let (root, engine) = engine().await;
    let file = root.path().join(DESCRIPTION_FILE);
    fs::write(&file, "notes.txt old\r\n").expect("write");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).expect("chmod");
    engine.dispatch(set("notes.txt", "new")).await.expect("set");
    let mode = fs::metadata(&file).expect("stat").permissions().mode();
    assert_eq!(mode & 0o7777, 0o640);
    assert_eq!(fs::read(&file).expect("read"), b"notes.txt new\r\n");
}
