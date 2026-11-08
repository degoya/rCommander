use crate::test_support::{self, expect_tabs, run_panel};
use crate::{Command, Engine, FileFilter, OpenTabRequest, Outcome, PanelId, SelectTabRequest};

/// `sub/` (with `inner.txt` and `tool.exe`), then `a.txt`, `b.exe`, `c.md`.
const TREE: &[(&str, &str)] = &[
    ("sub/inner.txt", "x"),
    ("sub/tool.exe", "x"),
    ("a.txt", "x"),
    ("b.exe", "x"),
    ("c.md", "x"),
];

fn names(engine: &Engine) -> Vec<String> {
    engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

fn file_filter(filter: FileFilter) -> Command {
    Command::SetFileFilter {
        panel: None,
        filter: Some(filter),
    }
}

async fn navigate(engine: &Engine, path: &str) {
    engine
        .dispatch(Command::Navigate {
            panel: None,
            path: path.into(),
        })
        .await
        .expect("navigates");
}

#[tokio::test]
async fn a_file_filter_survives_a_folder_change() {
    let (_root, engine) = test_support::engine(TREE).await;
    let snapshot = run_panel(&engine, file_filter(FileFilter::Programs)).await;
    assert_eq!(names(&engine), ["sub", "b.exe"]);
    assert_eq!(snapshot.entry_count, 2);
    navigate(&engine, "sub").await;
    assert_eq!(names(&engine), ["tool.exe"]);
    navigate(&engine, "..").await;
    assert_eq!(names(&engine), ["sub", "b.exe"]);
    engine
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refresh");
    assert_eq!(names(&engine), ["sub", "b.exe"], "a refresh keeps it");
}

#[tokio::test]
async fn folders_stay_visible_under_a_file_filter() {
    let (_root, engine) = test_support::engine(TREE).await;
    run_panel(&engine, file_filter(FileFilter::Mask("*.md".into()))).await;
    assert_eq!(names(&engine), ["sub", "c.md"]);
    // A blank mask is no filter.
    run_panel(&engine, file_filter(FileFilter::Mask("  ".into()))).await;
    assert_eq!(names(&engine).len(), 4);
    let error = engine
        .dispatch(file_filter(FileFilter::Mask("a\u{1}".into())))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "filter_invalid");
}

#[tokio::test]
async fn ctrl_f10_also_lifts_the_quick_filter() {
    let (_root, engine) = test_support::engine(TREE).await;
    run_panel(&engine, file_filter(FileFilter::Mask("*.txt".into()))).await;
    run_panel(
        &engine,
        Command::Filter {
            panel: None,
            pattern: Some("a*".into()),
            last: false,
        },
    )
    .await;
    assert_eq!(names(&engine), ["a.txt"], "both filters hold together");
    let snapshot = run_panel(&engine, file_filter(FileFilter::All)).await;
    assert_eq!(snapshot.filter, None);
    assert_eq!(names(&engine).len(), 4);
}

/// The tabs' rows of panel `Left`.
async fn tabs(engine: &Engine) -> Vec<crate::TabView> {
    expect_tabs(
        engine
            .dispatch(Command::ListTabs(crate::PanelRequest {
                panel: Some(PanelId::Left),
            }))
            .await
            .expect("tabs"),
    )
}

async fn select(engine: &Engine, tab: crate::TabId) {
    engine
        .dispatch(Command::SelectTab(SelectTabRequest { panel: None, tab }))
        .await
        .expect("selects");
}

#[tokio::test]
async fn the_filter_follows_the_tab_or_the_panel() {
    let (_root, engine) = test_support::engine_with_db(TREE).await;
    run_panel(&engine, file_filter(FileFilter::Programs)).await;
    engine
        .dispatch(Command::OpenTab(OpenTabRequest::default()))
        .await
        .expect("opens");
    assert_eq!(
        names(&engine),
        ["sub", "b.exe"],
        "a new tab shows what the panel showed"
    );
    run_panel(&engine, file_filter(FileFilter::Mask("*.txt".into()))).await;
    let rows = tabs(&engine).await;
    select(&engine, rows[0].id).await;
    assert_eq!(names(&engine), ["sub", "b.exe"], "each tab keeps its own");
    select(&engine, rows[1].id).await;
    assert_eq!(names(&engine), ["sub", "a.txt"]);

    // One filter per panel: the tabs share the last one set.
    engine
        .dispatch(Command::SetSetting {
            key: crate::file_filter::FILTER_PER_TAB_KEY.into(),
            value: serde_json::json!(false),
        })
        .await
        .expect("stored");
    run_panel(&engine, file_filter(FileFilter::Mask("*.md".into()))).await;
    select(&engine, rows[0].id).await;
    assert_eq!(names(&engine), ["sub", "c.md"]);
}

#[tokio::test]
async fn saved_filters_are_kept_by_name() {
    let (_root, engine) = test_support::engine_with_db(TREE).await;
    let saved = |outcome: Outcome| match outcome {
        Outcome::FileFilters { saved } => saved,
        other => panic!("filters, got {other:?}"),
    };
    let run = |command: Command| {
        let engine = engine.clone();
        async move { engine.dispatch(command).await }
    };
    run(Command::SaveFileFilter {
        name: "Text".into(),
        mask: " *.txt;*.md ".into(),
    })
    .await
    .expect("saved");
    let list = saved(
        run(Command::SaveFileFilter {
            name: "text".into(),
            mask: "*.txt".into(),
        })
        .await
        .expect("replaced"),
    );
    assert_eq!(list.len(), 1, "one name ignoring case");
    assert_eq!(list[0].mask, "*.txt");
    let list = saved(
        run(Command::RenameFileFilter {
            name: "TEXT".into(),
            new_name: "Docs".into(),
        })
        .await
        .expect("renamed"),
    );
    assert_eq!(list[0].name, "Docs");
    let error = run(Command::SaveFileFilter {
        name: " ".into(),
        mask: "*.x".into(),
    })
    .await
    .expect_err("no name");
    assert_eq!(error.code(), "saved_filter_invalid");
    let list = saved(
        run(Command::DeleteFileFilter {
            name: "docs".into(),
        })
        .await
        .expect("deleted"),
    );
    assert!(list.is_empty());
    let error = run(Command::DeleteFileFilter {
        name: "docs".into(),
    })
    .await
    .expect_err("gone");
    assert_eq!(error.code(), "saved_filter_not_found");
    assert!(saved(run(Command::ListFileFilters).await.expect("list")).is_empty());
}

#[test]
fn stored_filters_are_read_as_untrusted() {
    use super::{SavedFilter, sanitized};
    let filter = |name: &str, mask: &str| SavedFilter {
        name: name.into(),
        mask: mask.into(),
    };
    let kept = sanitized(vec![
        filter("b", "*.b"),
        filter("", "*.x"),
        filter("bad", ""),
        filter("A", "*.a"),
        filter("B", "*.other"),
        filter(&"n".repeat(65), "*.n"),
    ]);
    assert_eq!(kept, vec![filter("A", "*.a"), filter("b", "*.b")]);
}
