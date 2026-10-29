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
