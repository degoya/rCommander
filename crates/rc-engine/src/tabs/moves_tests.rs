use tempfile::TempDir;

use crate::test_support;
use crate::{
    Command, CursorTarget, Engine, LockTabRequest, Outcome, PanelId, PanelRequest, PanelView,
    VfsPath,
    tabs::requests::{HistoryJumpRequest, SwapRequest, TransferDirRequest},
};

async fn engine() -> (TempDir, Engine) {
    test_support::engine(&[("a/", ""), ("b/", ""), ("c/deep/", ""), ("file.txt", "x")]).await
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("command")
}

async fn go(engine: &Engine, panel: PanelId, path: &str) {
    run(
        engine,
        Command::Navigate {
            panel: Some(panel),
            path: path.into(),
        },
    )
    .await;
}

fn shown(engine: &Engine, panel: PanelId) -> VfsPath {
    engine.panel(panel).path
}

fn left() -> PanelRequest {
    PanelRequest {
        panel: Some(PanelId::Left),
    }
}

/// The acceptance check of RC-010-03: Alt+Left and Alt+Right walk the history; after going back,
/// a new folder drops the folders ahead.
#[tokio::test]
async fn back_forward_and_a_new_folder_after_back() {
    let (root, engine) = engine().await;
    let at = |dir: &str| VfsPath::local(root.path().join(dir));
    go(&engine, PanelId::Left, "a").await;
    go(&engine, PanelId::Left, "../b").await;
    run(&engine, Command::HistoryBack(left())).await;
    assert_eq!(shown(&engine, PanelId::Left), at("a"));
    run(&engine, Command::HistoryBack(left())).await;
    assert_eq!(shown(&engine, PanelId::Left), VfsPath::local(root.path()));
    let error = engine
        .dispatch(Command::HistoryBack(left()))
        .await
        .expect_err("nothing further back");
    assert_eq!(error.code(), "history_end");
    run(&engine, Command::HistoryForward(left())).await;
    assert_eq!(shown(&engine, PanelId::Left), at("a"));
    // A new folder now: `b`, which lay ahead, is gone from the history.
    go(&engine, PanelId::Left, "../c").await;
    let error = engine
        .dispatch(Command::HistoryForward(left()))
        .await
        .expect_err("nothing ahead");
    assert_eq!(error.code(), "history_end");
    let Outcome::History {
        entries, current, ..
    } = run(&engine, Command::HistoryList(left())).await
    else {
        panic!("a history");
    };
    assert_eq!(entries, [VfsPath::local(root.path()), at("a"), at("c")]);
    assert_eq!(current, 2);
}

#[tokio::test]
async fn a_jump_shows_any_folder_of_the_history() {
    let (root, engine) = engine().await;
    go(&engine, PanelId::Left, "a").await;
    go(&engine, PanelId::Left, "../b").await;
    let jump = |index| Command::HistoryJump(HistoryJumpRequest { panel: None, index });
    run(&engine, jump(0)).await;
    assert_eq!(shown(&engine, PanelId::Left), VfsPath::local(root.path()));
    run(&engine, Command::HistoryForward(left())).await;
    assert_eq!(
        shown(&engine, PanelId::Left),
        VfsPath::local(root.path().join("a"))
    );
    let error = engine.dispatch(jump(9)).await.expect_err("outside");
    assert_eq!(error.code(), "history_out_of_range");
}

/// A folder of the history that is gone leaves the history where it was.
#[tokio::test]
async fn a_gone_folder_keeps_the_place_in_the_history() {
    let (root, engine) = engine().await;
    go(&engine, PanelId::Left, "a").await;
    go(&engine, PanelId::Left, "..").await;
    std::fs::remove_dir(root.path().join("a")).expect("remove");
    assert!(engine.dispatch(Command::HistoryBack(left())).await.is_err());
    let Outcome::History { current, .. } = run(&engine, Command::HistoryList(left())).await else {
        panic!("a history");
    };
    assert_eq!(current, 2);
}

/// Ctrl+U swaps the folders; each panel keeps its view and its tab row.
#[tokio::test]
async fn ctrl_u_swaps_the_folders() {
    let (root, engine) = engine().await;
    go(&engine, PanelId::Left, "a").await;
    go(&engine, PanelId::Right, "b").await;
    run(
        &engine,
        Command::SwapPanels(SwapRequest { with_tabs: false }),
    )
    .await;
    assert_eq!(
        shown(&engine, PanelId::Left),
        VfsPath::local(root.path().join("b"))
    );
    assert_eq!(
        shown(&engine, PanelId::Right),
        VfsPath::local(root.path().join("a"))
    );
    assert_eq!(engine.panel(PanelId::Left).view, PanelView::Files);
    // The swap is a step of each tab's history.
    run(&engine, Command::HistoryBack(left())).await;
    assert_eq!(
        shown(&engine, PanelId::Left),
        VfsPath::local(root.path().join("a"))
    );
}

/// Ctrl+Shift+U swaps the tab rows: a locked tab moves with its panel's tabs.
#[tokio::test]
async fn ctrl_shift_u_swaps_the_tab_rows() {
    let (_root, engine) = engine().await;
    run(
        &engine,
        Command::LockTab(LockTabRequest {
            panel: Some(PanelId::Left),
            ..LockTabRequest::default()
        }),
    )
    .await;
    run(
        &engine,
        Command::SwapPanels(SwapRequest { with_tabs: true }),
    )
    .await;
    let Outcome::Tabs { tabs, .. } = run(
        &engine,
        Command::ListTabs(PanelRequest {
            panel: Some(PanelId::Right),
        }),
    )
    .await
    else {
        panic!("tabs");
    };
    assert!(tabs[0].locked);
}

/// Ctrl+Right shows the folder under the left panel's cursor in the right panel; on a file, the
/// left panel's own folder.
#[tokio::test]
async fn ctrl_left_and_right_show_the_cursor_folder_in_a_panel() {
    let (root, engine) = engine().await;
    let cursor = |name: &str| Command::SetCursor {
        panel: Some(PanelId::Left),
        target: CursorTarget::Name(name.into()),
    };
    let to = |side| {
        Command::TransferDir(TransferDirRequest {
            panel: Some(PanelId::Left),
            side,
            path: None,
        })
    };
    run(&engine, cursor("c")).await;
    run(&engine, to(PanelId::Right)).await;
    assert_eq!(
        shown(&engine, PanelId::Right),
        VfsPath::local(root.path().join("c"))
    );
    assert_eq!(shown(&engine, PanelId::Left), VfsPath::local(root.path()));
    run(&engine, cursor("file.txt")).await;
    run(&engine, to(PanelId::Right)).await;
    assert_eq!(shown(&engine, PanelId::Right), VfsPath::local(root.path()));
    run(&engine, cursor("a")).await;
    run(&engine, to(PanelId::Left)).await;
    assert_eq!(
        shown(&engine, PanelId::Left),
        VfsPath::local(root.path().join("a"))
    );
}
