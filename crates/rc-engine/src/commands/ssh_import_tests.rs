use std::{fs, path::Path, path::PathBuf};

use rc_db::Db;
use tempfile::TempDir;
use tokio::sync::broadcast::error::TryRecvError;

/// PEM armour, split so that no secret scanner takes the test for a leaked key.
const ARMOUR: &str = concat!("-----BEGIN ", "OPENSSH PRIVATE KEY-----\n");

use super::LocalUser;
use crate::test_support;
use crate::{
    AuthMethod, Command, ConnectionTree, Engine, EngineError, EngineEvent, ImportStatus, Outcome,
    SshConfigReport, TreeChange, TreeItem,
};

/// An engine and a home `<temp>/home/tester` whose `.ssh` holds the parser's fixture.
async fn setup() -> (TempDir, Engine, PathBuf) {
    let root = tempfile::tempdir().expect("temp dir");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ssh_config");
    let ssh = root.path().join("home/tester/.ssh");
    fs::create_dir_all(ssh.join("config.d")).expect("ssh dir");
    fs::copy(fixtures.join("config"), ssh.join("config")).expect("config");
    for entry in fs::read_dir(fixtures.join("config.d")).expect("config.d") {
        let entry = entry.expect("entry");
        fs::copy(entry.path(), ssh.join("config.d").join(entry.file_name())).expect("copy");
    }
    let engine = test_support::headless(root.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    (root, engine, ssh.join("config"))
}

fn import(path: &Path, dry_run: bool) -> Command {
    Command::ImportSshConfig(crate::SshConfigImportRequest {
        path: Some(path.display().to_string()),
        dry_run,
        hosts: None,
        folder_id: None,
    })
}

/// Runs an import command for the user `tester` whose `.ssh/config` is `config`.
async fn run(engine: &Engine, config: &Path, command: Command) -> Result<Outcome, EngineError> {
    let Command::ImportSshConfig(crate::SshConfigImportRequest {
        path,
        dry_run,
        hosts,
        folder_id,
    }) = command
    else {
        panic!("an import command, got {command:?}");
    };
    let user = LocalUser {
        home: config
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf),
        name: "tester".to_owned(),
    };
    engine
        .import_ssh_config_for(user, path.as_deref(), dry_run, hosts.as_deref(), folder_id)
        .await
}

async fn report(engine: &Engine, config: &Path, command: Command) -> SshConfigReport {
    match run(engine, config, command).await.expect("import") {
        Outcome::SshConfigImported(report) => report,
        other => panic!("an import report, got {other:?}"),
    }
}

async fn tree(engine: &Engine) -> ConnectionTree {
    match engine
        .dispatch(Command::ListConnections)
        .await
        .expect("list")
    {
        Outcome::ConnectionTree { tree } => tree,
        other => panic!("the tree, got {other:?}"),
    }
}

fn statuses(report: &SshConfigReport) -> Vec<&str> {
    report
        .hosts
        .iter()
        .map(|host| match host.status {
            ImportStatus::New => "new",
            ImportStatus::Created { .. } => "created",
            ImportStatus::Update { .. } => "update",
            ImportStatus::Updated { .. } => "updated",
            ImportStatus::Unchanged { .. } => "unchanged",
            ImportStatus::Refused { .. } => "refused",
        })
        .collect()
}

#[tokio::test]
async fn the_fixture_previews_exactly_its_hosts_and_reasons_and_writes_nothing() {
    let (_root, engine, config) = setup().await;
    let mut events = engine.subscribe();
    let preview = report(&engine, &config, import(&config, true)).await;

    let aliases: Vec<&str> = preview
        .hosts
        .iter()
        .map(|host| host.alias.as_str())
        .collect();
    assert_eq!(
        aliases,
        [
            "work-a", "pi", "web1", "web2", "db", "bastion", "internal", "git", "Mixed", "nas"
        ]
    );
    assert_eq!(statuses(&preview), ["new"; 10]);
    assert!(preview.hosts.iter().all(|host| host.selected));
    assert!(preview.dry_run);
    assert_eq!(preview.folder_id, None);
    assert!(!preview.folder_created);
    let internal = &preview.hosts[6];
    assert_eq!(internal.proxy_jump.as_deref(), Some("bastion"));
    assert_eq!(internal.ignored_options, ["ForwardAgent"]);
    let skipped: Vec<(&str, &str)> = preview
        .skipped
        .iter()
        .map(|skip| (skip.name.as_str(), skip.reason.as_str()))
        .collect();
    assert_eq!(
        skipped,
        [
            ("*", "pattern"),
            ("*.corp", "pattern"),
            ("!secret.corp", "negated")
        ]
    );
    let warnings: Vec<(&str, usize)> = preview
        .warnings
        .iter()
        .map(|warning| (warning.code.as_str(), warning.line))
        .collect();
    assert_eq!(warnings, [("match_skipped", 40)]);

    assert_eq!(tree(&engine).await, ConnectionTree::default());
    assert_eq!(
        events.try_recv(),
        Err(TryRecvError::Empty),
        "a dry run announces nothing"
    );
}

#[tokio::test]
async fn a_second_import_of_the_same_file_adds_no_duplicate() {
    let (_root, engine, config) = setup().await;
    let mut events = engine.subscribe();
    let first = report(&engine, &config, import(&config, false)).await;
    assert_eq!(statuses(&first), ["created"; 10]);
    assert!(first.folder_created);
    let folder = first.folder_id.expect("the default folder");
    assert_eq!(
        events.try_recv(),
        Ok(EngineEvent::ConnectionTree {
            item: TreeItem::Folder(folder),
            change: TreeChange::Created
        })
    );

    let second = report(&engine, &config, import(&config, false)).await;
    assert_eq!(statuses(&second), ["unchanged"; 10]);
    assert!(!second.folder_created);
    assert_eq!(second.folder_id, Some(folder));

    let tree = tree(&engine).await;
    assert_eq!(tree.folders.len(), 1);
    assert_eq!(tree.folders[0].folder.name, "Imported from ssh config");
    assert_eq!(tree.folders[0].connections.len(), 10);
    assert!(tree.connections.is_empty());
}

#[tokio::test]
async fn identity_files_land_as_paths_and_proxy_jump_in_the_notes() {
    let (root, engine, config) = setup().await;
    report(&engine, &config, import(&config, false)).await;
    let tree = tree(&engine).await;
    let by_name = |name: &str| {
        tree.folders[0]
            .connections
            .iter()
            .find(|connection| connection.name == name)
            .expect("imported")
            .clone()
    };
    let db = by_name("db");
    assert_eq!(db.auth_method, AuthMethod::Key);
    let key_path = root.path().join("home/tester/.ssh/id_db");
    assert_eq!(db.key_path, Some(key_path.display().to_string()));
    assert_eq!(db.port, 2222);
    let internal = by_name("internal");
    assert_eq!(internal.auth_method, AuthMethod::Agent);
    assert!(
        internal
            .notes
            .as_deref()
            .is_some_and(|notes| notes.contains("ProxyJump bastion")),
        "{:?}",
        internal.notes
    );
}

#[tokio::test]
async fn a_changed_host_is_offered_as_an_update_and_then_updated() {
    let (_root, engine, config) = setup().await;
    report(&engine, &config, import(&config, false)).await;
    let text = fs::read_to_string(&config).expect("read");
    fs::write(
        &config,
        text.replace("HostName nas.local", "HostName nas.home"),
    )
    .expect("write");

    let preview = report(&engine, &config, import(&config, true)).await;
    assert_eq!(statuses(&preview)[9], "update");
    let done = report(&engine, &config, import(&config, false)).await;
    assert_eq!(statuses(&done)[9], "updated");
    assert_eq!(statuses(&done)[..9], ["unchanged"; 9]);
}

#[tokio::test]
async fn only_the_named_hosts_are_written() {
    let (_root, engine, config) = setup().await;
    let done = report(
        &engine,
        &config,
        Command::ImportSshConfig(crate::SshConfigImportRequest {
            path: Some(config.display().to_string()),
            dry_run: false,
            hosts: Some(vec!["WEB1".into(), "nas".into()]),
            folder_id: None,
        }),
    )
    .await;
    let written: Vec<&str> = done
        .hosts
        .iter()
        .filter(|host| matches!(host.status, ImportStatus::Created { .. }))
        .map(|host| host.alias.as_str())
        .collect();
    assert_eq!(written, ["web1", "nas"]);
    assert_eq!(tree(&engine).await.folders[0].connections.len(), 2);
}

#[tokio::test]
async fn bad_input_is_refused_before_anything_is_written() {
    let (root, engine, config) = setup().await;
    let outside = root.path().join("config");
    fs::copy(&config, &outside).expect("copy");
    let code = |result: Result<Outcome, EngineError>| result.expect_err("refused").code();

    assert_eq!(
        code(run(&engine, &config, import(&outside, true)).await),
        "ssh_config_path_refused"
    );
    let climbing = config.join("../../../config");
    assert_eq!(
        code(run(&engine, &config, import(&climbing, true)).await),
        "ssh_config_path_refused"
    );
    assert_eq!(
        code(
            run(
                &engine,
                &config,
                import(&config.with_file_name("config.d").join("missing"), true)
            )
            .await
        ),
        "ssh_config_unreadable"
    );
    assert_eq!(
        code(
            run(
                &engine,
                &config,
                Command::ImportSshConfig(crate::SshConfigImportRequest {
                    path: Some(config.display().to_string()),
                    dry_run: false,
                    hosts: Some(vec!["web1".into(), "nowhere".into()]),
                    folder_id: None,
                })
            )
            .await
        ),
        "ssh_config_host_unknown"
    );
    assert_eq!(tree(&engine).await, ConnectionTree::default());
}

#[tokio::test]
async fn without_a_path_the_users_own_config_is_read() {
    let (_root, engine, config) = setup().await;
    let command = Command::ImportSshConfig(crate::SshConfigImportRequest {
        path: None,
        dry_run: true,
        hosts: None,
        folder_id: None,
    });
    let preview = report(&engine, &config, command).await;
    assert_eq!(preview.hosts.len(), 10);
}

/// Rule 1: only `~/.ssh/config` and `~/.ssh/config.d/*`, by canonical path - not another file
/// of `.ssh`, not a link out of it, not a missing file elsewhere (no existence oracle).
#[tokio::test]
async fn only_the_users_config_and_its_drop_ins_are_read() {
    let (root, engine, config) = setup().await;
    let ssh = config.parent().expect("ssh dir").to_path_buf();
    let code = |result: Result<Outcome, EngineError>| result.expect_err("refused").code();

    let drop_in = ssh.join("config.d/10-work.conf");
    let preview = report(&engine, &config, import(&drop_in, true)).await;
    assert_eq!(preview.hosts[0].alias, "work-a");

    fs::write(ssh.join("notes"), "Host leaked\n").expect("notes");
    assert_eq!(
        code(run(&engine, &config, import(&ssh.join("notes"), true)).await),
        "ssh_config_path_refused"
    );
    let elsewhere = root.path().join("nothing-here");
    assert_eq!(
        code(run(&engine, &config, import(&elsewhere, true)).await),
        "ssh_config_path_refused",
        "a missing file outside is refused like an existing one"
    );
    #[cfg(unix)]
    {
        let document = root.path().join("document.txt");
        fs::write(&document, "Host leaked\n").expect("document");
        std::os::unix::fs::symlink(&document, ssh.join("config.d/link")).expect("link");
        assert_eq!(
            code(run(&engine, &config, import(&ssh.join("config.d/link"), true)).await),
            "ssh_config_path_refused",
            "the canonical path decides"
        );
    }
}

/// Rule 2: key material stops the whole import, whether named like a key or holding one.
#[tokio::test]
async fn key_material_aborts_the_whole_import() {
    let (_root, engine, config) = setup().await;
    let drop_ins = config.with_file_name("config.d");
    let code = |result: Result<Outcome, EngineError>| result.expect_err("refused").code();

    fs::write(drop_ins.join("id_ed25519"), "Host leaked\n").expect("key");
    assert_eq!(
        code(run(&engine, &config, import(&drop_ins.join("id_ed25519"), true)).await),
        "ssh_config_key_material"
    );
    fs::write(
        drop_ins.join("30-stolen.conf"),
        format!("{ARMOUR}b3BlbnNzaC1rZXk=\n"),
    )
    .expect("key text");
    let error = run(&engine, &config, import(&config, false))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "ssh_config_key_material");
    assert!(!error.to_string().contains("b3BlbnNzaC1rZXk"), "{error}");
    assert_eq!(
        tree(&engine).await,
        ConnectionTree::default(),
        "nothing imported"
    );
}

/// Under MCP an import never moves a connection that holds a stored secret: the host is
/// reported refused, the row keeps its endpoint, and the person's own import updates it.
#[tokio::test]
async fn an_mcp_import_never_moves_a_connection_with_a_secret() {
    let (_root, engine, config) = setup().await;
    let first = report(&engine, &config, import(&config, false)).await;
    let nas = match &first.hosts[9].status {
        ImportStatus::Created { id } => *id,
        other => panic!("nas created, got {other:?}"),
    };
    engine
        .db()
        .expect("db")
        .set_connection_secret(
            nas,
            crate::SecretKind::Password,
            Some(b"ciphertext".as_slice()),
        )
        .await
        .expect("a stored secret");
    let text = fs::read_to_string(&config).expect("read");
    fs::write(
        &config,
        text.replace("HostName nas.local", "HostName attacker.example"),
    )
    .expect("write");

    let held = crate::read_guard::as_mcp(report(&engine, &config, import(&config, false))).await;
    assert!(
        matches!(&held.hosts[9].status, ImportStatus::Refused { code } if code == "endpoint_change_needs_gui"),
        "{:?}",
        held.hosts[9].status
    );
    let host = |engine: Engine| async move {
        match engine.dispatch(Command::GetConnection { id: nas }).await {
            Ok(Outcome::Connection { connection }) => connection.host,
            other => panic!("a connection, got {other:?}"),
        }
    };
    assert_eq!(host(engine.clone()).await, "nas.local");
    let done = report(&engine, &config, import(&config, false)).await;
    assert_eq!(statuses(&done)[9], "updated");
    assert_eq!(host(engine.clone()).await, "attacker.example");
}

/// RC-080-12 (X4): under MCP an `Include` is followed inside `.ssh` only; the window follows it
/// wherever the person's configuration points.
#[tokio::test]
async fn under_mcp_an_include_outside_ssh_is_refused() {
    let (root, engine, config) = setup().await;
    let outside = root.path().join("elsewhere.conf");
    fs::write(&outside, "Host far\n    HostName far.example\n").expect("write");
    fs::write(
        &config,
        format!(
            "Include {}\nHost near\n    HostName near.example\n",
            outside.display()
        ),
    )
    .expect("write");
    let aliases = |report: &SshConfigReport| -> Vec<String> {
        report.hosts.iter().map(|host| host.alias.clone()).collect()
    };

    let by_mcp = crate::read_guard::as_mcp(report(&engine, &config, import(&config, true))).await;
    assert_eq!(aliases(&by_mcp), ["near"]);
    assert!(
        by_mcp
            .warnings
            .iter()
            .any(|warning| warning.code == "include_refused"),
        "{:?}",
        by_mcp.warnings
    );
    let by_window = report(&engine, &config, import(&config, true)).await;
    assert_eq!(aliases(&by_window), ["far", "near"]);
}
