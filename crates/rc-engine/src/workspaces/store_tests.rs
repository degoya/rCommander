//! The stored workspaces through the commands: saved, read, changed, ordered and deleted, every
//! boundary checked, every change announced.

use rc_vfs::{BackendId, VfsPath};
use serde_json::json;
use tokio::sync::broadcast::error::TryRecvError;

use super::super::{TabSpec, spec::sample};
use crate::{
    Command, ConnectionDraft, ConnectionId, CreateWorkspaceRequest, Engine, EngineError,
    EngineEvent, Outcome, UpdateWorkspaceRequest, Workspace, WorkspaceId, WorkspaceSpec,
    WorkspacesOutcome, test_support,
};

fn outcome(result: Result<Outcome, EngineError>) -> WorkspacesOutcome {
    match result.expect("answered") {
        Outcome::Workspaces(outcome) => *outcome,
        other => panic!("a workspaces outcome, got {other:?}"),
    }
}

fn code(result: Result<Outcome, EngineError>) -> &'static str {
    result.expect_err("refused").code()
}

fn create(name: &str, spec: WorkspaceSpec) -> Command {
    Command::CreateWorkspace(CreateWorkspaceRequest {
        name: name.into(),
        spec,
        theme_light: None,
        theme_dark: None,
    })
}

async fn created(engine: &Engine, name: &str) -> WorkspaceId {
    match outcome(engine.dispatch(create(name, sample())).await) {
        WorkspacesOutcome::Saved { id } => id,
        other => panic!("saved, got {other:?}"),
    }
}

async fn listed(engine: &Engine) -> Vec<Workspace> {
    match outcome(engine.dispatch(Command::ListWorkspaces {}).await) {
        WorkspacesOutcome::List { workspaces } => workspaces,
        other => panic!("a list, got {other:?}"),
    }
}

async fn names(engine: &Engine) -> Vec<String> {
    listed(engine).await.into_iter().map(|w| w.name).collect()
}

async fn connection(engine: &Engine, name: &str) -> ConnectionId {
    match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new(name, "nas.example.org"),
        })
        .await
        .expect("created")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

fn changes(events: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> usize {
    let mut count = 0;
    loop {
        match events.try_recv() {
            Ok(EngineEvent::WorkspacesChanged) => count += 1,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(_) => return count,
        }
    }
}

#[tokio::test]
async fn a_workspace_is_saved_read_changed_and_deleted_and_each_change_announced() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    assert!(listed(&engine).await.is_empty());
    let id = created(&engine, "  Servers ").await;
    assert_eq!(changes(&mut events), 1);
    let read = match outcome(engine.dispatch(Command::GetWorkspace { id }).await) {
        WorkspacesOutcome::Workspace(workspace) => *workspace,
        other => panic!("a workspace, got {other:?}"),
    };
    assert_eq!(read.name, "Servers", "trimmed");
    assert_eq!(read.spec, sample());
    assert!(!read.active);
    assert_eq!(listed(&engine).await, std::slice::from_ref(&read));

    let mut spec = sample();
    spec.split_permille = Some(700);
    let update = UpdateWorkspaceRequest {
        id,
        spec: Some(spec.clone()),
        theme_dark: Some(Some("nord-dark".into())),
        ..UpdateWorkspaceRequest::default()
    };
    assert_eq!(
        outcome(engine.dispatch(Command::UpdateWorkspace(update)).await),
        WorkspacesOutcome::Done {}
    );
    let rename = Command::RenameWorkspace {
        id,
        name: "Web".into(),
    };
    outcome(engine.dispatch(rename).await);
    let after = &listed(&engine).await[0];
    assert_eq!(after.name, "Web");
    assert_eq!(after.spec, spec);
    assert_eq!(after.theme_dark.as_deref(), Some("nord-dark"));
    assert_eq!(after.theme_light, None);
    let cleared = UpdateWorkspaceRequest {
        id,
        theme_dark: Some(None),
        ..UpdateWorkspaceRequest::default()
    };
    outcome(engine.dispatch(Command::UpdateWorkspace(cleared)).await);
    assert_eq!(listed(&engine).await[0].theme_dark, None);
    assert_eq!(changes(&mut events), 3);

    outcome(engine.dispatch(Command::DeleteWorkspace { id }).await);
    assert!(listed(&engine).await.is_empty());
    assert_eq!(changes(&mut events), 1);
    for gone in [
        Command::GetWorkspace { id },
        Command::DeleteWorkspace { id },
        Command::MoveWorkspace { id, position: 0 },
        Command::ApplyWorkspace { id },
        Command::RenameWorkspace {
            id,
            name: "x".into(),
        },
    ] {
        assert_eq!(code(engine.dispatch(gone).await), "workspace_unknown");
    }
}

#[tokio::test]
async fn a_name_is_unique_without_regard_to_case_and_must_fit() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let first = created(&engine, "Servers").await;
    let other = created(&engine, "Local").await;
    assert_eq!(
        code(engine.dispatch(create("SERVERS", sample())).await),
        "workspace_name_taken"
    );
    let rename = Command::RenameWorkspace {
        id: other,
        name: "servers".into(),
    };
    assert_eq!(code(engine.dispatch(rename).await), "workspace_name_taken");
    let same = Command::RenameWorkspace {
        id: first,
        name: "SERVERS".into(),
    };
    outcome(engine.dispatch(same).await);
    for name in ["", " ", "a\nb"] {
        assert_eq!(
            code(engine.dispatch(create(name, sample())).await),
            "workspace_name"
        );
    }
    assert_eq!(names(&engine).await, ["SERVERS", "Local"]);
}

#[tokio::test]
async fn the_order_is_the_persons() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let a = created(&engine, "A").await;
    created(&engine, "B").await;
    let c = created(&engine, "C").await;
    outcome(
        engine
            .dispatch(Command::MoveWorkspace { id: c, position: 0 })
            .await,
    );
    assert_eq!(names(&engine).await, ["C", "A", "B"]);
    outcome(
        engine
            .dispatch(Command::MoveWorkspace { id: a, position: 9 })
            .await,
    );
    assert_eq!(names(&engine).await, ["C", "B", "A"]);
}

#[tokio::test]
async fn connections_must_exist_when_saved() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let server = connection(&engine, "nas").await;
    let mut spec = sample();
    spec.right.tabs.push(TabSpec {
        location: VfsPath {
            backend: BackendId::Ftp {
                connection_id: server.0,
            },
            path: "/pub".into(),
        },
        sort: None,
        mode: None,
        locked: false,
    });
    outcome(engine.dispatch(create("Nas", spec.clone())).await);
    let mut unknown = spec.clone();
    unknown.right.tabs[1].location.backend = BackendId::Ftp {
        connection_id: server.0 + 100,
    };
    assert_eq!(
        code(engine.dispatch(create("Other", unknown)).await),
        "workspace_connection"
    );
    let mut archive = sample();
    let outer = archive.left.tabs[0].location.join("a.zip");
    archive.left.tabs[0].location = VfsPath::archive(outer, rc_vfs::ArchiveFormat::Zip, "/");
    assert_eq!(
        code(engine.dispatch(create("Zip", archive)).await),
        "workspace_location"
    );
}

#[tokio::test]
async fn themes_must_exist_for_their_mode() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let themed = |light: &str, dark: &str| {
        Command::CreateWorkspace(CreateWorkspaceRequest {
            name: format!("{light} {dark}"),
            spec: sample(),
            theme_light: Some(light.into()),
            theme_dark: Some(dark.into()),
        })
    };
    outcome(engine.dispatch(themed("fluent-light", "fluent-dark")).await);
    assert_eq!(
        code(engine.dispatch(themed("fluent-dark", "fluent-dark")).await),
        "workspace_theme",
        "a dark theme for the light mode"
    );
    assert_eq!(
        code(engine.dispatch(themed("fluent-light", "neon")).await),
        "workspace_theme"
    );
    let blank = Command::CreateWorkspace(CreateWorkspaceRequest {
        name: "Blank".into(),
        spec: sample(),
        theme_light: Some("  ".into()),
        theme_dark: None,
    });
    outcome(engine.dispatch(blank).await);
    let blank = listed(&engine).await.pop().expect("there");
    assert_eq!(blank.theme_light, None, "an empty theme is none");
}

#[tokio::test]
async fn on_start_names_a_workspace_there_is() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    assert_eq!(engine.start_workspace().await, None);
    let id = created(&engine, "Start").await;
    let set = |value: serde_json::Value| Command::SetSetting {
        key: super::super::ON_START_KEY.into(),
        value,
    };
    engine
        .dispatch(set(json!(id.to_string())))
        .await
        .expect("set");
    assert_eq!(engine.start_workspace().await, Some(id));
    assert_eq!(
        code(engine.dispatch(set(json!((id + 1).to_string()))).await),
        "workspace_unknown"
    );
    engine.dispatch(set(json!(""))).await.expect("none");
    assert_eq!(engine.start_workspace().await, None);
}

#[tokio::test]
async fn a_row_that_no_longer_reads_is_left_out_of_the_list() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    created(&engine, "Good").await;
    engine
        .db()
        .expect("db")
        .create_workspace("Future", "{\"version\":9}", None, None)
        .await
        .expect("stored");
    assert_eq!(names(&engine).await, ["Good"]);
}

#[tokio::test]
async fn the_manager_is_the_windows() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::WorkspaceManager {})
        .await
        .expect("asked");
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::WorkspaceManager));
    assert_eq!(
        Command::WorkspaceManager {}.spec().class,
        crate::CommandClass::GuiOnly
    );
    assert_eq!(
        Command::ApplyWorkspace { id: 1 }.spec().class,
        crate::registry::spec("terminal.open").expect("a row").class,
        "applying may start shells"
    );
}
