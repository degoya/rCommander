//! The import page's keys (RC-050-06) through `apply` and a real engine: the engine reads only
//! the real `~/.ssh/config`, so the page gets its preview and its import as the engine's
//! answers. Without a ticked box no key is taken and nothing is asked; with one the master
//! password is asked once, the keys go into the vault, an encrypted key asks for its passphrase
//! in its row, a wrong one is said there, and a key on a share is refused in its row while the
//! others go on. The passphrase never shows in the page's `Debug`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use rc_db::Db;
use rc_engine::{
    Command, ConnectionDraft, ConnectionId, Drive, DriveGroup, DriveKind, DriveState, Engine,
    ImportStatus, Outcome, SshConfigHost, SshConfigReport, VfsPath,
};
use rc_vault::SecretString;

use super::{ImportDialog, ImportMsg, Phase, importable, imported_id, status_code, wsl_roots};
use crate::app::{
    Effect, Message,
    connections::{
        ConnJob, ConnMsg, ConnectionManager,
        jobs::Then,
        preview::ImportPage,
        vault::{AfterVault, Ask},
    },
    harness::Harness,
};

const RC_REMOTE_FIXTURES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../rc-remote/tests/fixtures");
const PASSPHRASE: &str = "fixture-passphrase";
const SHARE: &str = "//nas/keys/id_share";

#[test]
fn only_new_and_changed_hosts_are_offered_and_imported_ones_have_their_connection() {
    let id = ConnectionId(4);
    assert!(importable(&ImportStatus::New));
    assert!(importable(&ImportStatus::Update { id }));
    assert!(!importable(&ImportStatus::Unchanged { id }));
    assert!(!importable(&ImportStatus::Refused {
        code: "name_taken".into()
    }));
    assert_eq!(imported_id(&ImportStatus::Created { id }), Some(id));
    assert_eq!(imported_id(&ImportStatus::New), None);
    assert_eq!(status_code(&ImportStatus::Unchanged { id }), "unchanged");
}

fn drive(group: DriveGroup, label: &str, root: &str) -> Drive {
    Drive {
        id: root.to_owned(),
        group,
        kind: DriveKind::Wsl,
        label: label.to_owned(),
        root: Some(VfsPath::local(root)),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: None,
    }
}

#[test]
fn a_wsl_button_is_named_after_its_distribution_not_the_share_s_file_name() {
    // On Windows the root of a share has no file name, which left the buttons blank.
    let drives = [
        drive(DriveGroup::Drives, "C:", r"C:\"),
        drive(DriveGroup::Wsl, "Debian", r"\\wsl.localhost\Debian\"),
        drive(DriveGroup::Wsl, "", r"\\wsl$\Ubuntu-24.04\"),
    ];
    assert_eq!(
        wsl_roots(&drives),
        [
            (
                "Debian".to_owned(),
                PathBuf::from(r"\\wsl.localhost\Debian\")
            ),
            (
                "Ubuntu-24.04".to_owned(),
                PathBuf::from(r"\\wsl$\Ubuntu-24.04\")
            ),
        ]
    );
}

fn manager(harness: &Harness) -> &ConnectionManager {
    harness
        .app
        .connections
        .as_ref()
        .expect("the manager is open")
}

fn dialog(harness: &Harness) -> &ImportDialog {
    manager(harness).import.as_ref().expect("the page")
}

async fn send(harness: &mut Harness, message: ImportMsg) {
    harness
        .send(Message::Connections(ConnMsg::Import(message)))
        .await;
}

async fn answer(harness: &mut Harness, then: Then, report: SshConfigReport) {
    let answer = Box::new(Ok(Outcome::SshConfigImported(report)));
    harness
        .send(Message::Connections(ConnMsg::Answer(then, answer)))
        .await;
}

/// The page as it opens, before its first read.
fn page() -> ImportDialog {
    ImportDialog {
        path: String::new(),
        wsl: Vec::new(),
        folder: None,
        report: None,
        chosen: Vec::new(),
        take_key: Vec::new(),
        keys: Vec::new(),
        page: ImportPage::new(Phase::Reading),
    }
}

fn host(alias: &str, key: &str, status: ImportStatus) -> SshConfigHost {
    SshConfigHost {
        alias: alias.to_owned(),
        host: format!("{alias}.example.org"),
        port: None,
        user: None,
        identity_files: vec![key.to_owned()],
        proxy_jump: None,
        ignored_options: Vec::new(),
        file: "config".to_owned(),
        line: 1,
        selected: true,
        status,
    }
}

/// The engine's answer for the three hosts: the preview (`None`), or the import that created
/// these connections.
fn report(keys: &[(&str, String)], created: Option<&[ConnectionId]>) -> SshConfigReport {
    let hosts = keys
        .iter()
        .enumerate()
        .map(|(at, (alias, key))| {
            let status = match created {
                Some(ids) => ImportStatus::Created { id: ids[at] },
                None => ImportStatus::New,
            };
            host(alias, key, status)
        })
        .collect();
    SshConfigReport {
        path: "config".to_owned(),
        dry_run: created.is_none(),
        folder_id: None,
        folder_created: false,
        hosts,
        skipped: Vec::new(),
        warnings: Vec::new(),
    }
}

async fn create(engine: &Engine, name: &str) -> ConnectionId {
    let command = Command::CreateConnection {
        folder_id: None,
        connection: ConnectionDraft::new(name, "example.org"),
    };
    match engine.dispatch(command).await.expect("created") {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

async fn flags(engine: &Engine, id: ConnectionId) -> (bool, bool) {
    match engine
        .dispatch(Command::GetConnection { id })
        .await
        .expect("read")
    {
        Outcome::Connection { connection } => (
            connection.secrets.has_key,
            connection.secrets.has_passphrase,
        ),
        other => panic!("a connection, got {other:?}"),
    }
}

fn copy_key(fixture: &str, to: &Path) -> String {
    fs::copy(format!("{RC_REMOTE_FIXTURES}/{fixture}"), to).expect("key");
    to.display().to_string()
}

#[tokio::test]
async fn ticked_keys_go_into_the_vault_and_each_row_says_how() {
    let root = tempfile::tempdir().expect("temp dir");
    let keys = [
        (
            "plain",
            copy_key("client_key_ed25519", &root.path().join("id_plain")),
        ),
        (
            "locked",
            copy_key("client_key_rsapem", &root.path().join("id_locked")),
        ),
        ("share", SHARE.to_owned()),
    ];
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let mut ids = Vec::new();
    for (alias, _) in &keys {
        ids.push(create(&engine, alias).await);
    }
    let mut harness = Harness::new(engine.clone());
    harness.press("Ctrl+F").await;
    let mut page = page();
    let read = page.page.held.ask(String::new());
    harness.app.connections.as_mut().expect("open").import = Some(page);

    // Without a ticked box the import takes no key and asks for nothing.
    answer(&mut harness, Then::Preview(read), report(&keys, None)).await;
    assert_eq!(dialog(&harness).chosen, [true, true, true]);
    answer(&mut harness, Then::Imported, report(&keys, Some(&ids))).await;
    assert!(manager(&harness).prompt.is_none());
    assert!(dialog(&harness).keys.is_empty());

    // Ticked, the keys wait for the master password, the only time it is asked.
    answer(&mut harness, Then::Preview(read), report(&keys, None)).await;
    for at in 0..keys.len() {
        send(&mut harness, ImportMsg::TakeKey(at, true)).await;
    }
    answer(&mut harness, Then::Imported, report(&keys, Some(&ids))).await;
    let prompt = manager(&harness).prompt.as_ref().expect("the prompt");
    assert_eq!(prompt.ask, Ask::Create);
    assert_eq!(flags(&engine, ids[0]).await, (false, false));
    let master = || SecretString::from("master-password-1");
    for repeat in [false, true] {
        harness
            .send(Message::Connections(ConnMsg::VaultInput(repeat, master())))
            .await;
    }
    harness
        .send(Message::Connections(ConnMsg::VaultSubmit))
        .await;

    // Each row says what became of its key; the share's refusal does not stop the others.
    let page = dialog(&harness);
    let result = |at: usize| page.key_row(at).expect("a row").result;
    assert_eq!(result(0), Ok(()));
    assert_eq!(result(1), Err("key_passphrase_needed"));
    assert!(page.key_row(1).expect("a row").wants_passphrase());
    assert_eq!(result(2), Err("key_path_refused"));
    assert!(manager(&harness).note.as_ref().is_some_and(|note| note.1));
    assert_eq!(flags(&engine, ids[0]).await, (true, false));

    // A wrong passphrase is said in the row and asked again; an empty one sends nothing.
    send(
        &mut harness,
        ImportMsg::Passphrase(1, SecretString::from("not it")),
    )
    .await;
    send(&mut harness, ImportMsg::TakeAgain(1)).await;
    let row = dialog(&harness).key_row(1).expect("a row");
    assert_eq!(row.result, Err("key_passphrase_wrong"));
    send(&mut harness, ImportMsg::TakeAgain(1)).await;
    assert_eq!(
        dialog(&harness).key_row(1).expect("a row").result,
        Err("key_passphrase_wrong")
    );

    // The right one takes the key; the passphrase never shows in the page.
    send(
        &mut harness,
        ImportMsg::Passphrase(1, SecretString::from(PASSPHRASE)),
    )
    .await;
    let typed = format!("{:?}", manager(&harness));
    assert!(!typed.contains(PASSPHRASE), "{typed}");
    send(&mut harness, ImportMsg::TakeAgain(1)).await;
    assert_eq!(dialog(&harness).key_row(1).expect("a row").result, Ok(()));
    assert_eq!(flags(&engine, ids[1]).await, (true, true));
    assert_eq!(flags(&engine, ids[2]).await, (false, false));
    let shown = format!("{:?}", manager(&harness));
    assert!(!shown.contains(PASSPHRASE), "{shown}");
}

/// The ssh_config requests among `effects`, each with what its answer is for.
fn requests(effects: Vec<Effect>) -> Vec<(Then, rc_engine::SshConfigImportRequest)> {
    effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Connections(ConnJob::Run(then, command)) => match *command {
                Command::ImportSshConfig(request) => Some((then, request)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// A changed path drops the preview (RC-080-39): Import takes nothing from the new file, a
/// preview of the old one that answers late is left out, and the import takes the path its
/// preview was read from.
#[test]
fn a_changed_path_drops_the_preview_and_imports_nothing() {
    let keys = [("web", "id_web".to_owned())];
    let mut manager = ConnectionManager {
        import: Some(page()),
        ..ConnectionManager::default()
    };
    manager.on_import(ImportMsg::Path("/home/a/.ssh/config".into()));
    let read = requests(manager.on_import(ImportMsg::Read));
    let [(then, request)] = read.as_slice() else {
        panic!("one preview, got {read:?}");
    };
    assert_eq!(request.path.as_deref(), Some("/home/a/.ssh/config"));
    let preview = || Ok(Outcome::SshConfigImported(report(&keys, None)));
    manager.import_answer(*then, preview());
    assert_eq!(
        manager.import.as_ref().expect("page").page.phase,
        Phase::Ready
    );

    manager.on_import(ImportMsg::Path("/home/b/.ssh/config".into()));
    let dialog = manager.import.as_ref().expect("page");
    assert_eq!(dialog.page.phase, Phase::Done);
    assert!(dialog.report.is_none() && dialog.chosen.is_empty());
    assert!(requests(manager.on_import(ImportMsg::Run)).is_empty());
    manager.import_answer(*then, preview());
    assert!(manager.import.as_ref().expect("page").report.is_none());
    assert!(requests(manager.on_import(ImportMsg::Run)).is_empty());

    // Read again, the import takes the path of its preview.
    let read = requests(manager.on_import(ImportMsg::Read));
    let [(then, _)] = read.as_slice() else {
        panic!("one preview, got {read:?}");
    };
    manager.import_answer(*then, preview());
    let run = requests(manager.on_import(ImportMsg::Run));
    let [(Then::Imported, request)] = run.as_slice() else {
        panic!("the import, got {run:?}");
    };
    assert_eq!(request.path.as_deref(), Some("/home/b/.ssh/config"));
    assert_eq!(request.hosts.as_deref(), Some(&["web".to_owned()][..]));
}

/// A manager on the import page with the preview of `keys` read from one path.
fn previewed(keys: &[(&str, String)]) -> ConnectionManager {
    let mut manager = ConnectionManager {
        import: Some(page()),
        ..ConnectionManager::default()
    };
    manager.on_import(ImportMsg::Path("/home/a/.ssh/config".into()));
    let read = requests(manager.on_import(ImportMsg::Read));
    let [(then, _)] = read.as_slice() else {
        panic!("one preview, got {read:?}");
    };
    manager.import_answer(*then, Ok(Outcome::SshConfigImported(report(keys, None))));
    manager
}

/// RC-080-53: a preview read again that fails shows no hosts of the one before, and Import has
/// nothing to take.
#[test]
fn a_failed_preview_shows_no_hosts_of_the_one_before() {
    let mut manager = previewed(&[("web", "id_web".to_owned())]);
    assert!(manager.import.as_ref().expect("page").report.is_some());
    let read = requests(manager.on_import(ImportMsg::Read));
    let [(then, _)] = read.as_slice() else {
        panic!("one preview, got {read:?}");
    };
    manager.import_answer(*then, Err("ssh_config_unreadable"));
    let dialog = manager.import.as_ref().expect("page");
    assert!(dialog.report.is_none() && dialog.chosen.is_empty());
    assert!(requests(manager.on_import(ImportMsg::Run)).is_empty());
}

/// RC-080-53: the vault locked before the second key, not the first: the first key's row is
/// answered, the master password is asked, and only the second key is tried again.
#[test]
fn a_vault_locked_before_a_later_key_asks_and_tries_that_key_again() {
    let keys = [("web", "id_web".to_owned()), ("db", "id_db".to_owned())];
    let mut manager = previewed(&keys);
    manager.on_import(ImportMsg::TakeKey(0, true));
    manager.on_import(ImportMsg::TakeKey(1, true));
    let ids = [ConnectionId(1), ConnectionId(2)];
    manager.import_answer(
        Then::Imported,
        Ok(Outcome::SshConfigImported(report(&keys, Some(&ids)))),
    );
    manager.keys_taken(vec![
        ("web".to_owned(), Ok(())),
        ("db".to_owned(), Err("vault_locked")),
    ]);
    let prompt = manager.prompt.as_ref().expect("the vault is asked");
    assert_eq!(prompt.ask, Ask::Unlock);
    let AfterVault::TakeKeys(again) = &prompt.after else {
        panic!("the keys again, got {:?}", prompt.after);
    };
    let again: Vec<&str> = again.iter().map(|key| key.alias.as_str()).collect();
    assert_eq!(again, ["db"]);
    let rows = &manager.import.as_ref().expect("page").keys;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].alias.as_str(), rows[0].result), ("web", Ok(())));
}
