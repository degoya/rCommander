use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use rc_db::{DB_FILE, Db};
use rc_vault::{ExposeSecret as _, VaultStateKind};
use rc_vfs::VfsPath;
use tempfile::TempDir;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::test_support;
use crate::{
    Command, ConnectionKind, ConnectionTree, Engine, ImportStatus, Outcome, SecretKind,
    SecretParam, SecretRef, TcImportRequest, TcPasswordsRequest,
    import::{
        tc_password::TcPassword,
        total_commander_report::{TcImportReport, TcParts, TcRowStatus},
    },
    read_guard,
};

const MASTER: &str = "correct horse battery";
/// The password of "Plain FTP" in the fixture, and its obfuscated form there.
const CANARY: &str = "tc-canary-7f3a9e";
const CANARY_HEX: &str = "38A6F9D04C9E5111B21734ED77EA2FAA12345678";

/// An engine on a database file in a temp dir that also holds the fixtures as `wincmd.ini`
/// and `wcx_ftp.ini`; a vault that derives in milliseconds, not created yet.
async fn setup() -> (TempDir, Engine, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/total_commander");
    let tc = dir.path().join("tc");
    fs::create_dir(&tc).expect("tc dir");
    fs::copy(fixtures.join("wincmd_utf16.ini"), tc.join("wincmd.ini")).expect("wincmd");
    fs::copy(fixtures.join("wcx_ftp.ini"), tc.join("wcx_ftp.ini")).expect("wcx_ftp");
    let db = Db::open(dir.path().join(DB_FILE)).await.expect("db");
    let engine = test_support::headless(dir.path()).await;
    engine.attach_db(db.clone());
    let vault = test_support::fast_vault(db).await;
    assert!(engine.attach_vault(vault));
    (dir, engine, tc.join("wincmd.ini"))
}

/// The menu and the connections; the fixture's tabs are Windows folders.
fn request(wincmd: &Path, dry_run: bool) -> TcImportRequest {
    TcImportRequest {
        wincmd_ini: Some(wincmd.display().to_string()),
        parts: TcParts {
            tabs: false,
            ..TcParts::default()
        },
        dry_run,
        ..TcImportRequest::default()
    }
}

async fn import(engine: &Engine, request: TcImportRequest) -> TcImportReport {
    match engine
        .dispatch(Command::ImportTotalCommander(request))
        .await
        .expect("import")
    {
        Outcome::TcImported(report) => report,
        other => panic!("a report, got {other:?}"),
    }
}

async fn tree(engine: &Engine) -> ConnectionTree {
    match engine
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    {
        Outcome::ConnectionTree { tree } => tree,
        other => panic!("the tree, got {other:?}"),
    }
}

fn status_of<'a>(report: &'a TcImportReport, key: &str) -> &'a TcRowStatus {
    &report
        .hotlist
        .iter()
        .find(|row| row.key == key)
        .unwrap_or_else(|| panic!("no row {key}"))
        .status
}

/// Every log line written while the import runs, all levels, with its fields.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut line = String::new();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                let _ = write!(line, "{}={value:?} ", field.name());
            },
        );
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

#[tokio::test]
async fn a_preview_shows_everything_and_writes_nothing() {
    let (_dir, engine, wincmd) = setup().await;
    let report = import(&engine, request(&wincmd, true)).await;
    assert!(report.dry_run);
    assert!(
        report
            .ftp_ini
            .as_deref()
            .is_some_and(|path| path.ends_with("wcx_ftp.ini"))
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(*status_of(&report, "menu1"), TcRowStatus::New);
    assert_eq!(
        *status_of(&report, "menu5"),
        TcRowStatus::Skipped {
            reason: "program".to_owned()
        }
    );
    let kinds: Vec<(String, Option<ConnectionKind>, TcPassword)> = report
        .connections
        .iter()
        .map(|row| (row.key.clone(), row.kind, row.password_state))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (
                "ftp:plain ftp".into(),
                Some(ConnectionKind::Ftp),
                TcPassword::Present
            ),
            (
                "ftp:work|secure ftps".into(),
                Some(ConnectionKind::Ftps),
                TcPassword::Present
            ),
            (
                "ftp:work|servers|sftp box".into(),
                Some(ConnectionKind::Ssh),
                TcPassword::None
            ),
            (
                "ftp:locked".into(),
                Some(ConnectionKind::Ftp),
                TcPassword::MasterPassword
            ),
            (
                "ftp:anonymous".into(),
                Some(ConnectionKind::Ftp),
                TcPassword::None
            ),
            (
                "ftp:implicit".into(),
                Some(ConnectionKind::FtpsImplicit),
                TcPassword::None
            ),
            ("ftp:gopher".into(), None, TcPassword::None),
        ]
    );
    assert_eq!(
        report.connections[6].status,
        ImportStatus::Refused {
            code: "tc_protocol_unknown".to_owned()
        }
    );
    assert!(
        report.connections[..6]
            .iter()
            .all(|row| row.status == ImportStatus::New)
    );
    // Neither the password nor its obfuscated form is in the answer.
    let answer = serde_json::to_string(&report).expect("json");
    assert!(!answer.contains(CANARY) && !answer.to_uppercase().contains(CANARY_HEX));
    assert_eq!(tree(&engine).await, ConnectionTree::default());
    assert!(
        engine
            .db()
            .expect("db")
            .hotlist()
            .await
            .expect("hotlist")
            .is_empty()
    );
}

#[tokio::test]
async fn an_import_writes_once_and_a_second_one_adds_nothing() {
    let (_dir, engine, wincmd) = setup().await;
    let first = import(&engine, request(&wincmd, false)).await;
    for key in ["menu1", "menu2", "menu3"] {
        assert_eq!(*status_of(&first, key), TcRowStatus::Created, "{key}");
    }
    // `%TC_TEST_HOME%` is the parser tests' variable (`import/total_commander_tests.rs`); the
    // engine reads the real environment, where it is not set.
    let unknown = TcRowStatus::Skipped {
        reason: "unknown_variable".to_owned(),
    };
    assert_eq!(*status_of(&first, "menu12"), unknown);
    let hotlist = engine.db().expect("db").hotlist().await.expect("hotlist");
    let names: Vec<&str> = hotlist.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(names.len(), 4, "{names:?}");
    assert!(names.contains(&"Imported from Total Commander"));
    let before = tree(&engine).await;
    let root = &before.folders[0];
    assert_eq!(root.folder.name, "Imported from Total Commander");
    assert_eq!(root.connections.len(), 4);
    assert_eq!(root.folders[0].folder.name, "Work");
    assert_eq!(root.folders[0].folders[0].connections[0].name, "SFTP box");

    let second = import(&engine, request(&wincmd, false)).await;
    for key in ["menu1", "menu2", "menu3"] {
        assert_eq!(*status_of(&second, key), TcRowStatus::Exists, "{key}");
    }
    assert_eq!(*status_of(&second, "menu12"), unknown);
    assert!(
        second.connections[..6]
            .iter()
            .all(|row| matches!(row.status, ImportStatus::Unchanged { .. })),
        "{:?}",
        second.connections
    );
    assert_eq!(
        engine
            .db()
            .expect("db")
            .hotlist()
            .await
            .expect("again")
            .len(),
        4
    );
    assert_eq!(tree(&engine).await, before);
}

#[tokio::test]
async fn without_asking_no_password_is_read_and_no_master_password_wanted() {
    let (_dir, engine, wincmd) = setup().await;
    let lines = Lines::default();
    let report = {
        let _logs =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
        import(&engine, request(&wincmd, false)).await
    };
    let Ok(Outcome::Vault(status)) = engine.dispatch(Command::GetVaultStatus).await else {
        panic!("the vault's status");
    };
    assert_eq!(status.state, VaultStateKind::Absent);
    for row in &report.connections {
        if let ImportStatus::Created { id } = row.status {
            let stored = engine
                .resolve_secret(SecretRef {
                    connection: id,
                    kind: SecretKind::Password,
                })
                .await
                .expect("no vault needed");
            assert!(stored.is_none());
        }
    }
    let logged = lines
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .join("\n");
    assert!(
        !logged.contains(CANARY) && !logged.contains(CANARY_HEX),
        "{logged}"
    );
}

#[tokio::test]
async fn asked_for_the_passwords_go_only_into_the_vault() {
    let (dir, engine, wincmd) = setup().await;
    import(&engine, request(&wincmd, false)).await;
    let passwords = Command::ImportTotalCommanderPasswords(TcPasswordsRequest {
        wincmd_ini: Some(wincmd.display().to_string()),
        ..TcPasswordsRequest::default()
    });
    let absent = engine
        .dispatch(passwords.clone())
        .await
        .expect_err("no vault");
    assert_eq!(absent.code(), "vault_absent");
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault");

    let lines = Lines::default();
    let outcome = {
        let _logs =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
        engine.dispatch(passwords).await.expect("passwords")
    };
    let Outcome::TcPasswordsImported(report) = &outcome else {
        panic!("a password report, got {outcome:?}");
    };
    assert_eq!(
        report.stored,
        vec![
            "ftp:plain ftp".to_owned(),
            "ftp:work|secure ftps".to_owned()
        ]
    );
    let reason = |key: &str| {
        report
            .skipped
            .iter()
            .find(|skip| skip.key == key)
            .map(|skip| skip.reason.as_str())
    };
    assert_eq!(reason("ftp:locked"), Some("master_password"));
    assert_eq!(reason("ftp:work|servers|sftp box"), Some("none"));
    let answer = serde_json::to_string(&outcome).expect("json");
    let logged = lines
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .join("\n");
    for text in [&answer, &logged] {
        assert!(
            !text.contains(CANARY) && !text.contains(CANARY_HEX),
            "{text}"
        );
    }

    let plain = tree(&engine).await.folders[0]
        .connections
        .iter()
        .find(|connection| connection.name == "Plain FTP")
        .expect("Plain FTP")
        .id;
    let stored = engine
        .resolve_secret(SecretRef {
            connection: plain,
            kind: SecretKind::Password,
        })
        .await
        .expect("read")
        .expect("stored");
    assert_eq!(stored.expose_secret(), CANARY);
    for file in [DB_FILE, "rcommander.db-wal"] {
        let bytes = fs::read(dir.path().join(file)).unwrap_or_default();
        let found = bytes
            .windows(CANARY.len())
            .any(|window| window == CANARY.as_bytes());
        assert!(!found, "{file} holds the password in clear");
    }
}

#[tokio::test]
async fn over_mcp_a_dry_run_changes_nothing_and_the_passwords_stay_the_windows() {
    let (_dir, engine, wincmd) = setup().await;
    let preview = Command::ImportTotalCommander(request(&wincmd, true));
    let outcome = read_guard::as_mcp(engine.dispatch(preview))
        .await
        .expect("preview");
    assert!(matches!(outcome, Outcome::TcImported(_)));
    assert_eq!(tree(&engine).await, ConnectionTree::default());
    assert!(
        engine
            .db()
            .expect("db")
            .hotlist()
            .await
            .expect("hotlist")
            .is_empty()
    );

    let refused = engine
        .dispatch_mcp(Command::ImportTotalCommanderPasswords(
            TcPasswordsRequest::default(),
        ))
        .await
        .expect_err("gui only");
    assert_eq!(refused.code(), "gui_only");

    let elsewhere = wincmd.with_file_name("notes.ini");
    fs::copy(&wincmd, &elsewhere).expect("copy");
    let named = Command::ImportTotalCommander(request(&elsewhere, true));
    let refused = read_guard::as_mcp(engine.dispatch(named))
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "tc_ini_path_refused");
}

#[tokio::test]
async fn tabs_are_added_once_and_a_missing_folder_only_when_named() {
    let (dir, engine, _) = setup().await;
    let (here, there) = (dir.path().join("here"), dir.path().join("there"));
    fs::create_dir(&here).expect("here");
    fs::create_dir(&there).expect("there");
    let gone = dir.path().join("gone");
    let wincmd = dir.path().join("tabs").join("wincmd.ini");
    fs::create_dir(dir.path().join("tabs")).expect("tabs dir");
    fs::write(
        &wincmd,
        format!(
            "[left]\npath={}\n[lefttabs]\n0_path={}\n0_locked=1\n1_path={}\nactivetab=0\n",
            here.display(),
            there.display(),
            gone.display()
        ),
    )
    .expect("wincmd");
    let tabs_only = |dry_run: bool, only: Option<Vec<String>>| TcImportRequest {
        wincmd_ini: Some(wincmd.display().to_string()),
        parts: TcParts {
            hotlist: false,
            tabs: true,
            connections: false,
            look: false,
        },
        dry_run,
        only,
        ..TcImportRequest::default()
    };
    let preview = import(&engine, tabs_only(true, None)).await;
    let statuses: Vec<&TcRowStatus> = preview.tabs.iter().map(|row| &row.status).collect();
    assert_eq!(
        statuses,
        [&TcRowStatus::New, &TcRowStatus::New, &TcRowStatus::Missing]
    );
    assert!(preview.tabs[1].locked && preview.tabs[0].active);
    let before = engine.tabs(crate::PanelId::Left).len();

    import(&engine, tabs_only(false, None)).await;
    let after = engine.tabs(crate::PanelId::Left);
    assert_eq!(after.len(), before + 2);
    assert!(
        after
            .last()
            .is_some_and(|tab| tab.locked && tab.path == VfsPath::local(&there))
    );

    let again = import(&engine, tabs_only(true, None)).await;
    assert_eq!(again.tabs[0].status, TcRowStatus::Exists);
    let named = import(&engine, tabs_only(false, Some(vec!["left2".to_owned()]))).await;
    assert_eq!(named.tabs[2].status, TcRowStatus::Created);
    assert_eq!(engine.tabs(crate::PanelId::Left).len(), before + 3);
}
