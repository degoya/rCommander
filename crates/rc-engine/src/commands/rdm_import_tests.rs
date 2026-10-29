use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use rc_db::Db;
use rc_vfs::BackendId;
use tempfile::TempDir;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::test_support;
use crate::{
    AuthMethod, Command, Connection, ConnectionKind, ConnectionTree, Engine, FolderNode,
    ImportStatus, Outcome, RdmImportRequest,
    import::{
        rdm::RdmPassword,
        rdm_report::{DEFAULT_FOLDER, RdmImportReport},
    },
    read_guard::{self, RDM_EXPORT_PASSWORDS, ReadGuard},
};

/// The fixture's `SafePassword` and the one of its password history.
const CANARY: &str = "cmRtLWNhbmFyeS03ZjNhOQ==";
const HISTORY_CANARY: &str = "cmRtLWhpc3RvcnktYzRlMQ==";

/// An engine on an in-memory database, and the fixture copied into a temp dir as `export.rdm`.
async fn setup() -> (TempDir, Engine, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let export = dir.path().join("export.rdm");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rdm/sample.rdm"),
        &export,
    )
    .expect("fixture");
    let engine = test_support::headless(dir.path()).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    (dir, engine, export)
}

fn request(path: &Path, dry_run: bool) -> RdmImportRequest {
    RdmImportRequest {
        path: path.display().to_string(),
        dry_run,
        only: None,
    }
}

async fn import(engine: &Engine, request: RdmImportRequest) -> RdmImportReport {
    match engine
        .dispatch(Command::ImportRemoteDesktopManager(request))
        .await
        .expect("import")
    {
        Outcome::RdmImported(report) => report,
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

fn every_connection(tree: &ConnectionTree) -> Vec<Connection> {
    fn walk(folders: &[FolderNode], into: &mut Vec<Connection>) {
        for folder in folders {
            into.extend(folder.connections.iter().cloned());
            walk(&folder.folders, into);
        }
    }
    let mut all = tree.connections.clone();
    walk(&tree.folders, &mut all);
    all
}

fn connection<'a>(all: &'a [Connection], name: &str) -> &'a Connection {
    all.iter()
        .find(|connection| connection.name == name)
        .unwrap_or_else(|| panic!("no connection {name}"))
}

fn statuses(report: &RdmImportReport) -> Vec<(&str, &ImportStatus)> {
    report
        .entries
        .iter()
        .map(|row| (row.name.as_str(), &row.status))
        .collect()
}

fn refused(code: &str) -> ImportStatus {
    ImportStatus::Refused {
        code: code.to_owned(),
    }
}

#[tokio::test]
async fn the_preview_shows_every_entry_with_its_status_and_writes_nothing() {
    let (_dir, engine, export) = setup().await;
    let report = import(&engine, request(&export, true)).await;
    assert!(report.dry_run);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        statuses(&report),
        vec![
            ("build box", &ImportStatus::New),
            ("nested box", &ImportStatus::New),
            ("office box", &ImportStatus::New),
            ("file server", &refused("rdm_not_supported_yet")),
            ("windows desktop", &refused("rdm_not_supported")),
            ("no id", &refused("rdm_no_id")),
        ]
    );
    let build = &report.entries[0];
    assert_eq!(build.key, "0f3c1a52-7d4e-4b8a-9c61-2e5b8d7a4f10");
    assert_eq!(build.password_state, RdmPassword::Obfuscated);
    assert!(build.selected);
    assert_eq!(report.entries[5].key, "entry5");
    assert!(every_connection(&tree(&engine).await).is_empty());
}

#[tokio::test]
async fn the_import_builds_rdms_folders_and_takes_no_password() {
    let (_dir, engine, export) = setup().await;
    let report = import(&engine, request(&export, false)).await;
    let created = report
        .entries
        .iter()
        .filter(|row| matches!(row.status, ImportStatus::Created { .. }))
        .count();
    assert_eq!(created, 3);
    let tree = tree(&engine).await;
    let top = tree
        .folders
        .iter()
        .find(|folder| folder.folder.name == DEFAULT_FOLDER)
        .expect("the import's folder");
    let mut names: Vec<&str> = top
        .folders
        .iter()
        .map(|folder| folder.folder.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["Lab", "Office"]);
    let lab = top
        .folders
        .iter()
        .find(|folder| folder.folder.name == "Lab")
        .expect("Lab");
    assert_eq!(lab.connections[0].name, "build box");
    assert_eq!(lab.folders[0].folder.name, "Servers");
    assert_eq!(lab.folders[0].connections[0].name, "nested box");

    let all = every_connection(&tree);
    assert_eq!(all.len(), 3);
    let build = connection(&all, "build box");
    assert_eq!(
        (build.kind, build.host.as_str(), build.port),
        (ConnectionKind::Ssh, "build.example.test", 2222)
    );
    assert_eq!(build.username.as_deref(), Some("builder"));
    assert_eq!(build.auth_method, AuthMethod::Password);
    for connection in &all {
        assert!(!connection.secrets.has_password, "{}", connection.name);
    }
}

/// A key path makes key authentication; none, a password asked for when connecting.
#[test]
fn a_key_path_means_key_authentication() {
    let found = crate::import::rdm::parse(
        b"<RDMExport><Connections><Connection><ConnectionType>SSHShell</ConnectionType>\
          <ID>0a</ID><Name>k</Name><Terminal><Host>k.example.test</Host>\
          <PrivateKeyPath>C:/keys/id_ed25519</PrivateKeyPath></Terminal></Connection>\
          </Connections></RDMExport>",
    )
    .expect("parses");
    let entry = &found.entries[0];
    let with_key = super::draft(entry, ConnectionKind::Ssh);
    assert_eq!(with_key.auth_method, AuthMethod::Key);
    assert_eq!(with_key.key_path.as_deref(), Some("C:/keys/id_ed25519"));
    let without = super::draft(
        &crate::import::rdm::RdmEntry {
            key_path: None,
            ..entry.clone()
        },
        ConnectionKind::Ssh,
    );
    assert_eq!(without.auth_method, AuthMethod::Password);
}

#[tokio::test]
async fn a_second_import_adds_nothing_and_a_changed_host_is_an_update() {
    let (_dir, engine, export) = setup().await;
    import(&engine, request(&export, false)).await;
    let again = import(&engine, request(&export, false)).await;
    let importable: Vec<&ImportStatus> = again
        .entries
        .iter()
        .filter(|row| row.kind.is_some())
        .map(|row| &row.status)
        .collect();
    assert_eq!(importable.len(), 3);
    assert!(
        importable
            .iter()
            .all(|status| matches!(status, ImportStatus::Unchanged { .. })),
        "{:?}",
        statuses(&again)
    );
    assert_eq!(every_connection(&tree(&engine).await).len(), 3);

    // A later export under another name, the host of one entry changed.
    let later = export.with_file_name("later.rdm");
    let text = fs::read_to_string(&export).expect("export");
    fs::write(
        &later,
        text.replace("build.example.test", "build2.example.test"),
    )
    .expect("later");
    let preview = import(&engine, request(&later, true)).await;
    assert!(matches!(
        preview.entries[0].status,
        ImportStatus::Update { .. }
    ));
    assert!(matches!(
        preview.entries[1].status,
        ImportStatus::Unchanged { .. }
    ));
    let done = import(&engine, request(&later, false)).await;
    assert!(matches!(
        done.entries[0].status,
        ImportStatus::Updated { .. }
    ));
    let all = every_connection(&tree(&engine).await);
    assert_eq!(all.len(), 3);
    assert_eq!(connection(&all, "build box").host, "build2.example.test");
}

#[tokio::test]
async fn only_the_entries_named_are_imported() {
    let (_dir, engine, export) = setup().await;
    let report = import(
        &engine,
        RdmImportRequest {
            only: Some(vec!["0f3c1a52-7d4e-4b8a-9c61-2e5b8d7a4f10".to_owned()]),
            ..request(&export, false)
        },
    )
    .await;
    assert!(matches!(
        report.entries[0].status,
        ImportStatus::Created { .. }
    ));
    assert!(!report.entries[1].selected);
    assert_eq!(report.entries[1].status, ImportStatus::New);
    assert_eq!(every_connection(&tree(&engine).await).len(), 1);
}

#[tokio::test]
async fn a_protected_or_broken_export_is_refused_with_its_code() {
    let (dir, engine, _) = setup().await;
    let cases = [
        (
            "protected.rdm",
            "q83vEjRWeJAbCd+/AAECAwQFBgcICQoLDA0ODxA=",
            "rdm_encrypted",
        ),
        ("broken.rdm", "<RDMExport><Connections>", "rdm_invalid"),
        ("other.rdm", "<html/>", "rdm_not_an_export"),
    ];
    for (name, text, code) in cases {
        let path = dir.path().join(name);
        fs::write(&path, text).expect("file");
        let error = engine
            .dispatch(Command::ImportRemoteDesktopManager(request(&path, true)))
            .await
            .expect_err("refused");
        assert_eq!(error.code(), code, "{name}");
    }
    let missing = dir.path().join("missing.rdm");
    let error = engine
        .dispatch(Command::ImportRemoteDesktopManager(request(&missing, true)))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "rdm_unreadable");
}

#[tokio::test]
async fn over_mcp_only_a_file_ending_in_rdm_is_read() {
    let (dir, engine, export) = setup().await;
    let renamed = dir.path().join("export.xml");
    fs::copy(&export, &renamed).expect("copy");
    for path in [renamed, dir.path().join("missing.txt")] {
        let error = read_guard::as_mcp(engine.import_rdm(&request(&path, true)))
            .await
            .expect_err("refused");
        assert_eq!(error.code(), "rdm_path_refused", "{}", path.display());
    }
    let preview = read_guard::as_mcp(engine.import_rdm(&request(&export, true)))
        .await
        .expect("an .rdm file is read");
    assert!(matches!(preview, Outcome::RdmImported(_)));
}

/// The judge MCP runs under refuses every `.rdm` with the reason the import alone reads it
/// under; a file the judge refuses for another reason - a key named `.rdm` - stays refused.
#[test]
fn under_the_guard_the_import_reads_an_export_and_nothing_behind_its_name() {
    fn judge(path: &Path) -> Option<&'static str> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        if name == "key.rdm" {
            Some("a private key or key store")
        } else {
            name.ends_with(".rdm").then_some(RDM_EXPORT_PASSWORDS)
        }
    }
    let dir = tempfile::tempdir().expect("dir");
    let export = dir.path().join("export.rdm");
    let key = dir.path().join("key.rdm");
    for path in [&export, &key] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rdm/sample.rdm"),
            path,
        )
        .expect("fixture");
    }
    let guard = Some(ReadGuard::new(judge, &BackendId::Local));
    assert!(super::read(&export, Some(guard)).is_ok());
    let refused = super::read(&key, Some(guard)).expect_err("refused");
    assert_eq!(refused.code(), "rdm_path_refused");
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
async fn no_password_reaches_the_report_the_log_or_the_database() {
    let (_dir, engine, export) = setup().await;
    let lines = Lines::default();
    let subscriber = tracing_subscriber::registry().with(lines.clone());
    let _guard = tracing::subscriber::set_default(subscriber);
    let report = import(&engine, request(&export, false)).await;
    let mut shown = serde_json::to_string(&report).expect("json");
    let _ = write!(shown, "{report:?}");
    let _ = write!(shown, "{:?}", tree(&engine).await);
    shown.push_str(
        &lines
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .join("\n"),
    );
    for canary in [CANARY, HISTORY_CANARY] {
        assert!(!shown.contains(canary), "{canary} in {shown}");
    }
}

/// The owner's real export, run by the coordinator:
/// `RC_RDM_SAMPLE=<path> cargo nextest run -p rc-engine --run-ignored only rdm`. It holds one
/// SSH shell; the preview makes one new SSH connection with host, port and user, and nothing
/// shown carries its `SafePassword`.
#[tokio::test]
#[ignore = "needs RC_RDM_SAMPLE, the owner's own export"]
async fn the_owners_export_gives_one_new_ssh_connection() {
    let Ok(sample) = std::env::var("RC_RDM_SAMPLE") else {
        panic!("set RC_RDM_SAMPLE to the export");
    };
    let text = fs::read_to_string(&sample).expect("sample");
    let secrets: Vec<String> = text
        .split("<SafePassword>")
        .skip(1)
        .filter_map(|rest| rest.split("</SafePassword>").next())
        .map(str::to_owned)
        .collect();
    assert!(!secrets.is_empty(), "the sample keeps a SafePassword");
    let (_dir, engine, _) = setup().await;
    let report = import(&engine, request(Path::new(&sample), true)).await;
    assert_eq!(report.entries.len(), 1, "{:?}", statuses(&report));
    let row = &report.entries[0];
    assert_eq!(row.status, ImportStatus::New);
    assert_eq!(row.kind, Some(ConnectionKind::Ssh));
    assert!(!row.host.is_empty() && row.port.is_some() && row.user.is_some());
    assert_eq!(row.password_state, RdmPassword::Obfuscated);
    let shown = format!(
        "{report:?}{}",
        serde_json::to_string(&report).expect("json")
    );
    for secret in &secrets {
        assert!(!shown.contains(secret.as_str()), "a SafePassword is shown");
    }
}
