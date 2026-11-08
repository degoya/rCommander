use crate::{
    Engine, StartItemKind, UserCommandDraft,
    import::{
        tc_ini,
        total_commander_commands::{TcCommand, TcStartEntry, commands, start_menu},
        total_commander_report::{DEFAULT_FOLDER, TcRowStatus},
    },
    read_guard, test_support,
};

const USERCMD: &str = "\
[em_np]
cmd=notepad.exe
param=\"%P%N\"
menu=Notepad
[em_reg]
cmd=*regedit.exe
[em_cd]
cmd=cd C:\\
";

const USER: &str = "\
[user]
menu1=Notepad
cmd1=notepad.exe
menu2=-
menu3=-Admin
menu4=Registry
cmd4=*regedit.exe
menu5=--
";

fn found() -> (Vec<TcCommand>, Vec<TcStartEntry>) {
    (
        commands(&tc_ini::parse(USERCMD)),
        start_menu(&tc_ini::parse(USER)),
    )
}

fn skipped(reason: &str) -> TcRowStatus {
    TcRowStatus::Skipped {
        reason: reason.to_owned(),
    }
}

async fn import(engine: &Engine, dry_run: bool) -> (Vec<TcRowStatus>, Vec<TcRowStatus>) {
    let (own, menu) = found();
    let commands = engine
        .import_tc_commands(&own, None, dry_run)
        .await
        .expect("commands planned");
    let start = engine
        .import_tc_start_menu(&menu, None, dry_run)
        .await
        .expect("Start menu planned");
    (
        commands.into_iter().map(|row| row.status).collect(),
        start.into_iter().map(|row| row.status).collect(),
    )
}

#[tokio::test]
async fn the_preview_writes_nothing_then_the_import_writes_once() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let (commands, start) = import(&engine, true).await;
    assert_eq!(
        commands,
        [TcRowStatus::New, TcRowStatus::New, skipped("change_dir")]
    );
    assert_eq!(start, vec![TcRowStatus::New; 4]);
    assert!(engine.user_commands().await.expect("read").is_empty());
    assert!(engine.start_menu().await.expect("read").is_empty());

    let (commands, start) = import(&engine, false).await;
    assert_eq!(
        commands,
        [
            TcRowStatus::Created,
            TcRowStatus::Created,
            skipped("change_dir")
        ]
    );
    assert_eq!(start, vec![TcRowStatus::Created; 4]);
    let stored = engine.user_commands().await.expect("read");
    let names: Vec<&str> = stored.iter().map(|command| command.name.as_str()).collect();
    assert_eq!(names, ["em_np", "em_reg"]);
    assert!(stored[1].action.elevated);
    assert_eq!(stored[0].tooltip, "Notepad");

    // One submenu at the top with Total Commander's menu inside, its submenu kept.
    let menu = engine.start_menu().await.expect("read");
    let shape: Vec<(usize, StartItemKind, &str)> = menu
        .iter()
        .map(|item| (item.depth, item.kind, item.title.as_str()))
        .collect();
    assert_eq!(
        shape,
        [
            (0, StartItemKind::Submenu, DEFAULT_FOLDER),
            (1, StartItemKind::Item, "Notepad"),
            (1, StartItemKind::Separator, ""),
            (1, StartItemKind::Submenu, "Admin"),
            (2, StartItemKind::Item, "Registry"),
        ]
    );
    assert!(
        menu[4]
            .action
            .as_ref()
            .is_some_and(|action| action.elevated)
    );

    // A second import adds nothing twice.
    let (commands, start) = import(&engine, false).await;
    assert_eq!(
        commands,
        [
            TcRowStatus::Exists,
            TcRowStatus::Exists,
            skipped("change_dir")
        ]
    );
    assert_eq!(start, vec![TcRowStatus::Exists; 4]);
    assert_eq!(engine.start_menu().await.expect("read").len(), 5);
}

#[tokio::test]
async fn a_changed_command_is_updated_and_only_what_is_named_goes_in() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let (own, menu) = found();
    let mut changed = own[0].draft.clone().expect("em_np");
    changed.action.params = "%S".to_owned();
    engine
        .store_user_command(None, &changed)
        .await
        .expect("stored");
    let only = ["em_np".to_owned(), "start1".to_owned()];
    let rows = engine
        .import_tc_commands(&own, Some(&only), false)
        .await
        .expect("imported");
    assert_eq!(rows[0].status, TcRowStatus::Updated);
    assert_eq!(rows[1].status, TcRowStatus::New, "not named: left out");
    assert!(!rows[1].selected);
    let stored = engine.user_commands().await.expect("read");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].action.params, "\"%P%N\"");

    let rows = engine
        .import_tc_start_menu(&menu, Some(&only), false)
        .await
        .expect("imported");
    assert_eq!(rows[0].status, TcRowStatus::Created);
    assert_eq!(rows[3].status, TcRowStatus::New);
    let titles: Vec<String> = engine
        .start_menu()
        .await
        .expect("read")
        .into_iter()
        .map(|item| item.title)
        .collect();
    assert_eq!(titles, [DEFAULT_FOLDER, "Notepad"]);
}

/// Which program runs is a person's choice: over MCP nothing is written, the preview says why.
#[tokio::test]
async fn over_mcp_own_commands_and_the_start_menu_need_the_window() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let (own, menu) = found();
    let existing = UserCommandDraft {
        name: "em_reg".to_owned(),
        ..own[1].draft.clone().expect("em_reg")
    };
    engine
        .store_user_command(None, &existing)
        .await
        .expect("stored");
    let rows = read_guard::as_mcp(engine.import_tc_commands(&own, None, false))
        .await
        .expect("planned");
    assert_eq!(rows[0].status, skipped("needs_gui"));
    assert!(!rows[0].selected);
    assert_eq!(rows[1].status, TcRowStatus::Exists, "nothing to change");
    let rows = read_guard::as_mcp(engine.import_tc_start_menu(&menu, None, false))
        .await
        .expect("planned");
    assert!(rows.iter().all(|row| row.status == skipped("needs_gui")));
    assert_eq!(engine.user_commands().await.expect("read").len(), 1);
    assert!(engine.start_menu().await.expect("read").is_empty());
}
