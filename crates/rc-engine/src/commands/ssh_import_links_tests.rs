//! ProxyJump and LocalForward of an ssh_config import (RC-110-03).

use std::{fs, path::Path};

use rc_db::Db;

use super::super::ssh_import::LocalUser;
use crate::test_support;
use crate::{Command, Connection, Engine, ForwardsOutcome, Outcome, SshConfigReport};

const CONFIG: &str = "\
Host bastion
    HostName bastion.example.org
    User jump

Host web
    HostName web.internal
    User deploy
    ProxyJump bastion
    LocalForward 0.0.0.0:8080 web:80
    LocalForward 15432 db:5432

Host far
    HostName far.internal
    ProxyJump j1,j2
";

/// An engine and a home `<temp>/home/tester` whose `.ssh/config` is [`CONFIG`].
async fn setup() -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().expect("temp dir");
    let ssh = root.path().join("home/tester/.ssh");
    fs::create_dir_all(&ssh).expect("ssh dir");
    fs::write(ssh.join("config"), CONFIG).expect("config");
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    (root, engine)
}

async fn import(engine: &Engine, root: &Path, dry_run: bool) -> SshConfigReport {
    let user = LocalUser {
        home: Some(root.join("home/tester")),
        name: "tester".to_owned(),
    };
    match engine
        .import_ssh_config_for(user, None, dry_run, None, None)
        .await
        .expect("import")
    {
        Outcome::SshConfigImported(report) => report,
        other => panic!("a report, got {other:?}"),
    }
}

async fn connection(engine: &Engine, name: &str) -> Connection {
    let Ok(Outcome::ConnectionTree { tree }) = engine.dispatch(Command::ListConnections).await
    else {
        panic!("the tree");
    };
    tree.folders[0]
        .connections
        .iter()
        .find(|connection| connection.name == name)
        .cloned()
        .expect("imported")
}

async fn forwards(engine: &Engine, connection: &Connection) -> Vec<String> {
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
        .inspect(|view| assert!(!view.forward.auto_start, "never started by an import"))
        .map(|view| view.label)
        .collect()
}

#[tokio::test]
async fn proxy_jump_sets_the_jump_host_and_a_list_stays_a_note() {
    let (root, engine) = setup().await;
    let report = import(&engine, root.path(), false).await;
    let bastion = connection(&engine, "bastion").await;
    let web = connection(&engine, "web").await;
    assert_eq!(web.jump_id, Some(bastion.id));
    assert_eq!(report.hosts[1].jump_id, Some(bastion.id));
    let far = connection(&engine, "far").await;
    assert_eq!(far.jump_id, None);
    assert!(
        far.notes
            .as_deref()
            .is_some_and(|notes| notes.contains("ProxyJump j1,j2 (not imported)")),
        "{:?}",
        far.notes
    );
}

#[tokio::test]
async fn local_forwards_come_in_from_the_window_lowered_to_loopback_once() {
    let (root, engine) = setup().await;
    let preview = import(&engine, root.path(), true).await;
    assert_eq!(
        preview.hosts[1].local_forwards,
        [
            "127.0.0.1:8080 \u{2192} web:80",
            "127.0.0.1:15432 \u{2192} db:5432"
        ]
    );
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning.code == "forward_bind_lowered"),
        "{:?}",
        preview.warnings
    );
    import(&engine, root.path(), false).await;
    let web = connection(&engine, "web").await;
    let expected = [
        "127.0.0.1:8080 \u{2192} web:80",
        "127.0.0.1:15432 \u{2192} db:5432",
    ];
    assert_eq!(forwards(&engine, &web).await, expected);
    // A second import adds none twice.
    import(&engine, root.path(), false).await;
    assert_eq!(forwards(&engine, &web).await, expected);
}

#[tokio::test]
async fn an_mcp_import_creates_no_forward_and_says_so() {
    let (root, engine) = setup().await;
    let report = crate::read_guard::as_mcp(import(&engine, root.path(), false)).await;
    assert!(report.hosts[1].forwards_gui_only);
    let web = connection(&engine, "web").await;
    assert!(forwards(&engine, &web).await.is_empty());
    assert_eq!(web.jump_id, Some(connection(&engine, "bastion").await.id));
}
