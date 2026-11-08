//! The class the MCP gate gives `usercmd.run` and `start_menu.run` from what is stored.

use crate::{
    Command, CommandClass, CommandKind, CommandRef, Engine, StartItemDraft, StartItemKind,
    UserCommandDraft, test_support,
};

async fn save(engine: &Engine, name: &str, action: CommandRef) {
    engine
        .store_user_command(
            None,
            &UserCommandDraft {
                name: name.into(),
                action,
                ..UserCommandDraft::default()
            },
        )
        .await
        .expect("saved");
}

fn program(params: &str, elevated: bool) -> CommandRef {
    CommandRef {
        command: "tool".into(),
        params: params.into(),
        elevated,
        ..CommandRef::default()
    }
}

fn rows(rows: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Internal,
        command: rows.into(),
        ..CommandRef::default()
    }
}

fn run(name: &str, params: Option<&str>) -> Command {
    Command::RunUserCommand {
        name: name.into(),
        panel: None,
        params: params.map(str::to_owned),
    }
}

#[tokio::test]
async fn what_is_stored_makes_a_run_the_persons() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    save(&engine, "em_plain", program("%N", false)).await;
    save(&engine, "em_admin", program("", true)).await;
    save(&engine, "em_ask", program("?%N", false)).await;
    save(&engine, "em_quit", rows("panel.refresh,app.quit")).await;
    save(&engine, "em_refresh", rows("panel.refresh,cm_SrcFull")).await;
    save(
        &engine,
        "em_via",
        CommandRef {
            kind: CommandKind::Own,
            command: "em_admin".into(),
            ..CommandRef::default()
        },
    )
    .await;
    save(&engine, "em_rows_via", rows("panel.refresh,em_ask")).await;
    for (name, class) in [
        ("em_plain", CommandClass::Exec),
        ("em_refresh", CommandClass::Exec),
        ("em_missing", CommandClass::Exec),
        ("em_admin", CommandClass::GuiOnly),
        ("em_ask", CommandClass::GuiOnly),
        ("em_quit", CommandClass::GuiOnly),
        ("em_via", CommandClass::GuiOnly),
        ("em_rows_via", CommandClass::GuiOnly),
    ] {
        assert_eq!(
            engine.effective_class(&run(name, None)).await,
            class,
            "{name}"
        );
    }
    assert_eq!(
        engine.effective_class(&run("em_plain", Some("x"))).await,
        CommandClass::GuiOnly,
        "parameters from the caller"
    );
}

#[tokio::test]
async fn a_start_menu_entry_is_judged_by_its_command() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut ids = Vec::new();
    for elevated in [false, true] {
        let id = engine
            .store_start_item(
                None,
                None,
                &StartItemDraft {
                    kind: StartItemKind::Item,
                    title: "Registry".into(),
                    action: Some(program("", elevated)),
                },
            )
            .await
            .expect("saved");
        ids.push(id);
    }
    let class = |id: i64| {
        let engine = engine.clone();
        async move {
            engine
                .effective_class(&Command::RunStartItem {
                    id,
                    panel: None,
                    params: None,
                })
                .await
        }
    };
    assert_eq!(class(ids[0]).await, CommandClass::Exec);
    assert_eq!(class(ids[1]).await, CommandClass::GuiOnly);
}
