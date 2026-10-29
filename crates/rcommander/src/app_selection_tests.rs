//! The file list's selection and views through `apply` (RC-010-02): Total Commander's `..` row
//! and keys, brief and full views, sorting, select and unselect group, Shift+arrows.

use std::fs;

use rc_engine::{PanelId, SortKey};

use super::{
    Message,
    harness::Harness,
    tests::{cursor_name, path, tree},
};

/// Total Commander's `..` row: Enter there goes up, Num* never selects it, and Ctrl+\ and
/// Ctrl+R work as there.
#[tokio::test]
async fn the_parent_row_and_the_total_commander_keys() {
    let root = tree();
    let mut harness = Harness::headless(&root.path().join("inner")).await;
    assert!(harness.app.pane(PanelId::Left).on_parent());
    harness.press("NumMultiply").await;
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(pane.selected_count(), 2);
    assert!(!pane.is_selected(0), "`..` is never selected");
    harness.press("Ctrl+NumSubtract").await;
    assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 0);
    harness.press("Ctrl+A").await;
    assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 2);

    fs::write(root.path().join("inner").join("new.txt"), b"x").expect("write");
    harness.press("Ctrl+R").await;
    assert_eq!(harness.app.pane(PanelId::Left).entries().len(), 3);
    assert!(
        harness.app.pane(PanelId::Left).on_parent(),
        "a reread stays on `..`"
    );

    harness.press("Enter").await;
    assert_eq!(path(&harness, PanelId::Left), root.path());
    assert_eq!(cursor_name(&harness, PanelId::Left), "inner");
    harness.press("Ctrl+\\").await;
    let at_root = path(&harness, PanelId::Left).to_path_buf();
    assert_eq!(at_root.parent(), None, "{}", at_root.display());
    assert!(!harness.app.pane(PanelId::Left).has_parent_row());
}

/// RC-010-02: Ctrl+F1 and Ctrl+F2 switch one panel between brief and full, Ctrl+F6 and a click
/// on a column header sort it.
#[tokio::test]
async fn views_and_sorting_belong_to_one_panel() {
    let root = tree();
    fs::write(root.path().join("f05.txt"), [0; 4096]).expect("write");
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+F1").await;
    assert!(harness.app.pane(PanelId::Left).brief());
    assert!(!harness.app.pane(PanelId::Right).brief());
    harness.press("Ctrl+F6").await;
    let names: Vec<&str> = harness
        .app
        .pane(PanelId::Left)
        .entries()
        .iter()
        .map(|entry| entry.name.as_str())
        .take(3)
        .collect();
    assert_eq!(names, ["inner", "outer", "f05.txt"], "largest first");
    harness.press("Ctrl+F2").await;
    assert!(!harness.app.pane(PanelId::Left).brief());

    harness
        .send(Message::SortBy(PanelId::Right, SortKey::Name))
        .await;
    let right = &harness.app.pane(PanelId::Right).state.sort;
    assert!(right.descending, "the same key again turns it around");
    assert_eq!(cursor_name(&harness, PanelId::Right), "inner");
}

/// Num+ asks for a mask in the panel; the keys typed meanwhile run nothing, and the Enter that
/// submits it opens nothing.
#[tokio::test]
async fn num_plus_selects_by_the_mask_typed_into_its_prompt() {
    let root = tree();
    fs::write(root.path().join("notes.md"), b"x").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    harness.press("NumAdd").await;
    let prompt = harness.app.prompt.clone().expect("a prompt");
    assert_eq!(prompt.text, "*.*");
    harness.press("ArrowDown").await;
    assert!(
        harness.app.pane(PanelId::Left).on_parent(),
        "keys go to the prompt"
    );
    harness.send(Message::PromptInput("*.md".to_owned())).await;
    harness.send(Message::PromptSubmit).await;
    harness
        .send(Message::KeyCaptured("Enter".parse().expect("a chord")))
        .await;
    assert!(harness.app.prompt.is_none());
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(
        pane.state.selection.iter().collect::<Vec<_>>(),
        ["notes.md"]
    );
    assert_eq!(path(&harness, PanelId::Left), root.path());

    harness.press("NumSubtract").await;
    assert_eq!(
        harness.app.prompt.as_ref().map(|p| p.text.as_str()),
        Some("*.md")
    );
    harness
        .send(Message::KeyCaptured("Escape".parse().expect("a chord")))
        .await;
    assert!(harness.app.prompt.is_none());
    assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 1);
}

/// Owner 2026-10-17, TC behaviour: Num+ "Select group" and Num- "Unselect group" match files
/// only unless a mask ends in `\`; the masks are kept, newest first, and ArrowUp walks them.
/// Alt+Num+ and Alt+Num- take every file with the cursor file's extension.
#[tokio::test]
async fn select_group_and_unselect_group_keep_their_masks() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    for (keys, mask) in [
        ("NumAdd", "f0*.txt"),
        ("NumAdd", "*\\"),
        ("NumSubtract", "*.*|f00.txt"),
    ] {
        harness.press(keys).await;
        harness.send(Message::PromptInput(mask.to_owned())).await;
        // Enter that no field took (the field lost the keyboard to a click) submits too.
        harness.press("Enter").await;
    }
    let pane = harness.app.pane(PanelId::Left);
    let mut selected: Vec<&str> = pane.state.selection.iter().map(String::as_str).collect();
    selected.sort_unstable();
    assert_eq!(
        selected,
        ["f00.txt", "inner", "outer"],
        "directories only by `\\`"
    );
    assert_eq!(
        path(&harness, PanelId::Left),
        root.path(),
        "Enter opened nothing"
    );

    harness.press("NumAdd").await;
    let text = |harness: &Harness| harness.app.prompt.as_ref().map(|p| p.text.clone());
    assert_eq!(text(&harness).as_deref(), Some("*.*|f00.txt"));
    harness.press("ArrowUp").await;
    assert_eq!(text(&harness).as_deref(), Some("*\\"));
    harness.press("ArrowUp").await;
    harness.press("ArrowUp").await;
    assert_eq!(
        text(&harness).as_deref(),
        Some("f0*.txt"),
        "the oldest stays"
    );
    harness.press("ArrowDown").await;
    assert_eq!(text(&harness).as_deref(), Some("*\\"));
    harness
        .send(Message::PromptRecall("f0*.txt".to_owned()))
        .await;
    assert_eq!(text(&harness).as_deref(), Some("f0*.txt"));
    harness.press("Escape").await;
    assert!(harness.app.prompt.is_none());

    harness.press("Ctrl+NumSubtract").await;
    harness.send(Message::Click(PanelId::Left, 5)).await;
    harness.press("Alt+NumAdd").await;
    assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 40);
    harness.press("Alt+NumSubtract").await;
    assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 0);
}

#[tokio::test]
async fn shift_arrows_select_and_the_status_line_sums_the_selection() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    // From `..`: onto `inner` unselected, then `inner`, `outer` and `f00.txt` selected.
    for keys in [
        "Shift+ArrowDown",
        "Shift+ArrowDown",
        "Shift+ArrowDown",
        "Shift+ArrowDown",
    ] {
        harness.press(keys).await;
    }
    let sums = *harness.app.pane(PanelId::Left).summary();
    assert_eq!(
        (sums.selected_dirs, sums.selected_files, sums.selected_bytes),
        (2, 1, 1)
    );
    assert_eq!((sums.dirs, sums.files, sums.bytes), (2, 40, 40));
    harness.press("NumMultiply").await;
    harness.press("NumDivide").await;
    assert_eq!(
        harness.app.pane(PanelId::Left).selected_count(),
        39,
        "no file operation took a selection yet"
    );
}

#[tokio::test]
async fn the_path_bar_goes_to_a_segment_or_to_a_typed_path() {
    let root = tree();
    let mut harness = Harness::headless(&root.path().join("inner").join("deeper")).await;
    harness
        .send(Message::PathSegment(
            PanelId::Left,
            root.path().join("inner"),
        ))
        .await;
    assert_eq!(path(&harness, PanelId::Left), root.path().join("inner"));
    harness.send(Message::EditPath(PanelId::Right)).await;
    let typed = root.path().join("outer").to_string_lossy().into_owned();
    harness.send(Message::PromptInput(typed)).await;
    harness.send(Message::PromptSubmit).await;
    assert_eq!(path(&harness, PanelId::Right), root.path().join("outer"));
}

#[tokio::test]
async fn the_status_line_switch_shows_hidden_files() {
    let root = tree();
    fs::write(root.path().join(".dotfile"), b"x").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    assert_eq!(harness.app.pane(PanelId::Left).entries().len(), 42);
    harness.send(Message::ToggleHidden).await;
    for panel in PanelId::BOTH {
        assert_eq!(harness.app.pane(panel).entries().len(), 43);
        assert!(harness.app.pane(panel).state.show_hidden);
    }
}

/// The owner's test list (RC-080-14, the Ctrl+R of f29b730): Ctrl+B shows the branch flat,
/// Ctrl+R brings the folder with its subfolders back - in the engine and on screen.
#[tokio::test]
async fn ctrl_r_leaves_the_branch_view_for_its_folder() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+B").await;
    for _ in 0..500 {
        let panel = harness.app.engine().panel(PanelId::Left);
        if panel.flat.as_ref().is_some_and(|flat| !flat.loading) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    harness.settle().await;
    let nested = std::path::Path::new("inner").join("file.txt");
    let shown = |harness: &Harness| -> Vec<String> {
        let state = &harness.app.pane(PanelId::Left).state;
        state
            .entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    };
    assert!(harness.app.pane(PanelId::Left).state.flat.is_some());
    assert!(shown(&harness).contains(&nested.display().to_string()));

    harness.press("Ctrl+R").await;
    harness.settle().await;
    assert!(harness.app.engine().panel(PanelId::Left).flat.is_none());
    let pane = harness.app.pane(PanelId::Left);
    assert!(pane.state.flat.is_none(), "the folders are back on screen");
    assert_eq!(path(&harness, PanelId::Left), root.path());
    assert!(shown(&harness).contains(&"inner".to_owned()));
    assert!(!shown(&harness).contains(&nested.display().to_string()));
}
