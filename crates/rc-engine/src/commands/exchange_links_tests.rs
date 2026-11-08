//! An export and its import keep each connection's jump host and port forwards (RC-110-03).

use crate::{
    Command, Connection, ConnectionDraft, Engine, ForwardDraft, ForwardsOutcome, Outcome,
    SaveForwardRequest,
    commands::exchange_fixture::{engine_in, export, import, report},
    exchange::ExportParts,
};

const CONNECTIONS: ExportParts = ExportParts {
    settings: false,
    connections: true,
    trust: false,
    hotlist: false,
};

async fn create(engine: &Engine, name: &str, jump: Option<crate::ConnectionId>) -> Connection {
    let draft = ConnectionDraft {
        username: Some("me".to_owned()),
        jump_id: jump,
        ..ConnectionDraft::new(name, format!("{name}.example.org"))
    };
    match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    {
        Ok(Outcome::Connection { connection }) => connection,
        other => panic!("a connection: {other:?}"),
    }
}

async fn by_name(engine: &Engine, name: &str) -> Connection {
    let tree = engine
        .db()
        .expect("db")
        .connection_tree()
        .await
        .expect("tree");
    tree.connections
        .into_iter()
        .find(|connection| connection.name == name)
        .expect("there")
}

async fn labels(engine: &Engine, connection: &Connection) -> Vec<(String, bool)> {
    let Ok(Outcome::Forwards(outcome)) = engine
        .dispatch(Command::ListForwards {
            connection_id: Some(connection.id),
        })
        .await
    else {
        panic!("forwards");
    };
    let ForwardsOutcome::Listed { forwards } = *outcome else {
        panic!("a list");
    };
    forwards
        .into_iter()
        .map(|view| (view.label, view.forward.auto_start))
        .collect()
}

#[tokio::test]
async fn an_export_and_its_import_keep_jump_hosts_and_forwards() {
    let source = tempfile::tempdir().expect("dir");
    let engine = engine_in(source.path()).await;
    let bastion = create(&engine, "bastion", None).await;
    let target = create(&engine, "target", Some(bastion.id)).await;
    engine
        .dispatch(Command::SaveForward(SaveForwardRequest {
            connection_id: target.id,
            id: None,
            forward: ForwardDraft {
                bind_address: "::1".to_owned(),
                bind_port: 15432,
                target_host: "db".to_owned(),
                target_port: 5432,
                auto_start: true,
                name: Some("prod".to_owned()),
            },
        }))
        .await
        .expect("a forward");
    let path = source.path().join("connections.rcexport");
    engine
        .dispatch(export(&path, CONNECTIONS, None, false))
        .await
        .expect("exported");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        !text.contains("\"jump_id\""),
        "no id of this installation travels"
    );

    let there = tempfile::tempdir().expect("dir");
    let other = engine_in(there.path()).await;
    let preview = report(&other, import(&path, None, true, None)).await;
    assert!(
        preview
            .rows
            .iter()
            .all(|row| row.warning.as_deref() != Some("jump_missing")),
        "the jump host is in the file"
    );
    report(&other, import(&path, None, false, None)).await;
    let bastion_there = by_name(&other, "bastion").await;
    let target_there = by_name(&other, "target").await;
    assert_eq!(target_there.jump_id, Some(bastion_there.id));
    let expected = vec![("[::1]:15432 \u{2192} db:5432".to_owned(), true)];
    assert_eq!(labels(&other, &target_there).await, expected);
    // A second import adds no forward twice.
    report(&other, import(&path, None, false, None)).await;
    assert_eq!(labels(&other, &target_there).await, expected);
}

#[tokio::test]
async fn a_jump_host_the_file_lacks_comes_in_as_none_with_a_warning() {
    let source = tempfile::tempdir().expect("dir");
    let engine = engine_in(source.path()).await;
    let bastion = create(&engine, "bastion", None).await;
    create(&engine, "target", Some(bastion.id)).await;
    let path = source.path().join("connections.rcexport");
    engine
        .dispatch(export(&path, CONNECTIONS, None, false))
        .await
        .expect("exported");
    // The file without the jump host's own entry.
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let list = json["content"]["connections"]["connections"]
        .as_array_mut()
        .expect("connections");
    list.retain(|entry| entry["connection"]["name"] != "bastion");
    std::fs::write(&path, json.to_string()).expect("written");

    let there = tempfile::tempdir().expect("dir");
    let other = engine_in(there.path()).await;
    let preview = report(&other, import(&path, None, true, None)).await;
    assert!(
        preview
            .rows
            .iter()
            .any(|row| row.warning.as_deref() == Some("jump_missing")),
        "{:?}",
        preview.rows
    );
    report(&other, import(&path, None, false, None)).await;
    assert_eq!(by_name(&other, "target").await.jump_id, None);
}
