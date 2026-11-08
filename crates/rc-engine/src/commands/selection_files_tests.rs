use std::fs;

use crate::read_guard::as_mcp;
use crate::test_support;
use crate::{Command, Engine, Outcome, PanelId};

const TREE: &[(&str, &str)] = &[
    ("sub/", ""),
    ("a.txt", "a"),
    ("b.md", "b"),
    ("c d.txt", "c"),
];

fn selected(engine: &Engine) -> Vec<String> {
    engine
        .panel(PanelId::Left)
        .selection
        .iter()
        .cloned()
        .collect()
}

async fn select(engine: &Engine, names: &[&str]) {
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        })
        .await
        .expect("selects");
}

fn loaded(outcome: Outcome) -> (usize, usize) {
    match outcome {
        Outcome::SelectionLoaded {
            selected, missing, ..
        } => (selected, missing),
        other => panic!("a loaded selection, got {other:?}"),
    }
}

#[tokio::test]
async fn selection_round_trips_through_a_file() {
    let (root, engine) = test_support::engine(TREE).await;
    select(&engine, &["sub", "c d.txt"]).await;
    let written = engine
        .dispatch(Command::SelectionToFile {
            panel: None,
            path: "mark.txt".into(),
            overwrite: false,
        })
        .await
        .expect("written");
    assert!(matches!(
        written,
        Outcome::SelectionWritten { names: 2, .. }
    ));
    let text = fs::read_to_string(root.path().join("mark.txt")).expect("file");
    assert_eq!(text.lines().collect::<Vec<_>>(), ["sub", "c d.txt"]);
    engine
        .dispatch(Command::DeselectAll { panel: None })
        .await
        .expect("cleared");
    let counts = loaded(
        engine
            .dispatch(Command::SelectionFromFile {
                panel: None,
                path: "mark.txt".into(),
            })
            .await
            .expect("loaded"),
    );
    assert_eq!(counts, (2, 0));
    assert_eq!(selected(&engine), ["c d.txt", "sub"]);
}

#[tokio::test]
async fn missing_names_are_counted() {
    let (root, engine) = test_support::engine(TREE).await;
    let text = format!(
        "A.TXT\n\"gone.txt\"\n{}\n",
        root.path().join("b.md").display()
    );
    let counts = loaded(
        engine
            .dispatch(Command::SelectionFromText { panel: None, text })
            .await
            .expect("loaded"),
    );
    assert_eq!(
        counts,
        (2, 1),
        "case ignored, a full path below the folder taken"
    );
    assert_eq!(selected(&engine), ["a.txt", "b.md"]);
}

#[tokio::test]
async fn an_existing_file_is_not_overwritten_over_mcp() {
    let (root, engine) = test_support::engine(TREE).await;
    fs::write(root.path().join("mark.txt"), "keep").expect("there");
    select(&engine, &["a.txt"]).await;
    let to_file = |overwrite| Command::SelectionToFile {
        panel: None,
        path: "mark.txt".into(),
        overwrite,
    };
    let error = engine
        .dispatch(to_file(false))
        .await
        .expect_err("there already");
    assert_eq!(error.code(), "file_exists");
    let error = as_mcp(engine.selection_to_file(None, "mark.txt", true))
        .await
        .expect_err("never over MCP");
    assert_eq!(error.code(), "file_exists");
    assert_eq!(
        fs::read_to_string(root.path().join("mark.txt")).expect("file"),
        "keep"
    );
    // The window asked, and the person said yes.
    engine.dispatch(to_file(true)).await.expect("overwritten");
    assert_eq!(
        fs::read_to_string(root.path().join("mark.txt"))
            .expect("file")
            .trim_end(),
        "a.txt"
    );
}

#[tokio::test]
async fn a_saved_selection_comes_back_with_num_slash() {
    let (_root, engine) = test_support::engine(TREE).await;
    select(&engine, &["a.txt", "b.md"]).await;
    engine
        .dispatch(Command::SaveSelection { panel: None })
        .await
        .expect("saved");
    engine
        .dispatch(Command::DeselectAll { panel: None })
        .await
        .expect("cleared");
    engine
        .dispatch(Command::RestoreSelection { panel: None })
        .await
        .expect("restored");
    assert_eq!(selected(&engine), ["a.txt", "b.md"]);
}
