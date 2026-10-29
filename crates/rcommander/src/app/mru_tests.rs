use rc_db::Db;

use super::{CMDLINE, MASKS, SEARCH};

#[test]
fn the_newest_mask_comes_first_once_and_the_oldest_go() {
    let mut history = Vec::new();
    for mask in ["*.txt", "*.md", "*.txt"] {
        MASKS.remember(&mut history, mask);
    }
    assert_eq!(history, ["*.txt", "*.md"]);
    for i in 0..30 {
        MASKS.remember(&mut history, &format!("*.{i}"));
    }
    assert_eq!(history.len(), MASKS.limit);
    assert_eq!(history[0], "*.29");
    MASKS.remember(&mut history, "  ");
    MASKS.remember(&mut history, &"x".repeat(2_000));
    assert_eq!(history[0], "*.29", "blank and overlong masks are not kept");
}

#[test]
fn the_newest_line_comes_first_once_and_the_oldest_go() {
    let mut history = Vec::new();
    // The command line trims a line before it keeps it.
    for line in ["dir", "ls -la", "  dir  "] {
        CMDLINE.remember(&mut history, line.trim());
    }
    assert_eq!(history, ["dir", "ls -la"]);
    for i in 0..40 {
        CMDLINE.remember(&mut history, &format!("echo {i}"));
    }
    assert_eq!(history.len(), CMDLINE.limit);
    assert_eq!(history[0], "echo 39");
    CMDLINE.remember(&mut history, "   ");
    CMDLINE.remember(&mut history, &"x".repeat(3_000));
    CMDLINE.remember(&mut history, "dir\ndel *");
    assert_eq!(
        history[0], "echo 39",
        "blank, overlong and broken lines are not kept"
    );
}

#[test]
fn a_stored_history_is_cleaned_before_use() {
    let mut stored: Vec<String> =
        vec!["*.a".into(), String::new(), "*.a".into(), "x".repeat(5_000)];
    stored.extend((0..40).map(|i| format!("*.{i}")));
    let history = MASKS.sanitized(stored);
    assert_eq!(history.len(), MASKS.limit);
    assert_eq!(&history[..2], ["*.a", "*.0"]);

    let mut stored: Vec<String> = vec![
        "make".into(),
        String::new(),
        "make".into(),
        "x".repeat(5_000),
        "a\u{0}b".into(),
    ];
    stored.extend((0..40).map(|i| format!("echo {i}")));
    let history = CMDLINE.sanitized(stored);
    assert_eq!(history.len(), CMDLINE.limit);
    assert_eq!(&history[..2], ["make", "echo 0"]);
}

/// The lists keep the settings and limits they had before they shared one type, so stored
/// values still read.
#[test]
fn each_list_keeps_its_setting_and_limit() {
    assert_eq!((CMDLINE.key, CMDLINE.limit), ("cmdline.history", 30));
    assert_eq!((MASKS.key, MASKS.limit), ("panels.mask_history", 20));
    assert_eq!((SEARCH.key, SEARCH.limit), ("search.history", 20));
}

#[tokio::test]
async fn the_history_survives_in_one_setting() {
    let db = Db::open_in_memory().await.expect("db");
    assert!(MASKS.load(&db).await.is_empty());
    MASKS
        .save(&db, &["*.rs".to_owned(), "*.md".to_owned()])
        .await
        .expect("saved");
    assert_eq!(MASKS.load(&db).await, ["*.rs", "*.md"]);
    assert!(SEARCH.load(&db).await.is_empty(), "each list has its own");
    db.set(MASKS.key, &"not a list").await.expect("set");
    assert!(
        MASKS.load(&db).await.is_empty(),
        "a damaged value is ignored"
    );
}

/// The history in a database file, and the same file opened again as at the next start.
#[tokio::test]
async fn the_history_survives_a_restart() {
    let folder = tempfile::tempdir().expect("temp dir");
    let file = folder.path().join("rcommander.db");
    {
        let db = Db::open(&file).await.expect("db");
        assert!(CMDLINE.load(&db).await.is_empty());
        CMDLINE
            .save(&db, &["git status".to_owned(), "cd ..".to_owned()])
            .await
            .expect("saved");
        db.close().await;
    }
    let again = Db::open(&file).await.expect("db again");
    assert_eq!(CMDLINE.load(&again).await, ["git status", "cd .."]);
    again.set(CMDLINE.key, &"not a list").await.expect("set");
    assert!(
        CMDLINE.load(&again).await.is_empty(),
        "a damaged value is ignored"
    );
}
