//! What the engine's unit tests share (RC-080-24): an engine over a temporary folder, with or
//! without a database and a vault, the folder's entries, and the outcome a command must answer.

use std::{fs, path::Path, sync::Arc, time::Duration};

use rc_db::{DB_FILE, Db, SECRET_COLUMNS, SecretColumn};
use rc_ops::JobSnapshot;
use rc_vault::{KdfParams, ManualClock, Vault, VaultOptions};
use rc_vfs::VfsPath;
use tempfile::TempDir;

use crate::{
    Command, ConnectionId, ConnectionPatch, Drive, Engine, Outcome, PanelSnapshot,
    SaveConnectionRequest, SaveFields, SecretChange, SecretKind, SecretParam, TabView,
};

/// A temporary folder holding `entries`, in order: a name ending in `/` is a folder, any other
/// a file with the text beside it; missing parents are created.
pub(crate) fn tree(entries: &[(&str, &str)]) -> TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    for (name, text) in entries {
        let path = root.path().join(name);
        if name.ends_with('/') {
            fs::create_dir_all(&path).expect("mkdir");
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&path, text).expect("write");
    }
    root
}

/// A new file `path` holding `text`, hidden as the platform hides it: a leading dot does on Unix,
/// on Windows only the hidden attribute does (a dot marks nothing there, as in Total Commander).
pub(crate) fn hidden_file(path: &Path, text: &str) {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        std::os::windows::fs::OpenOptionsExt::attributes(&mut options, FILE_ATTRIBUTE_HIDDEN);
    }
    let mut file = options.open(path).expect("create");
    std::io::Write::write_all(&mut file, text.as_bytes()).expect("write");
}

/// A headless engine on `root`, both panels there.
pub(crate) async fn headless(root: &Path) -> Engine {
    Engine::headless(VfsPath::local(root))
        .await
        .expect("engine")
}

/// [`headless`] with an in-memory database attached.
pub(crate) async fn headless_with_db(root: &Path) -> Engine {
    let engine = headless(root).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    engine
}

/// An engine with the left panel on `left` and the right one on `right`.
pub(crate) async fn open(left: &Path, right: &Path) -> Engine {
    Engine::open(VfsPath::local(left), VfsPath::local(right))
        .await
        .expect("engine")
}

/// A [`tree`] of `entries` and a [`headless`] engine on it.
pub(crate) async fn engine(entries: &[(&str, &str)]) -> (TempDir, Engine) {
    let root = tree(entries);
    let engine = headless(root.path()).await;
    (root, engine)
}

/// A [`tree`] of `entries` and a [`headless_with_db`] engine on it.
pub(crate) async fn engine_with_db(entries: &[(&str, &str)]) -> (TempDir, Engine) {
    let root = tree(entries);
    let engine = headless_with_db(root.path()).await;
    (root, engine)
}

/// Vault options whose key derives in milliseconds, on `clock`'s time.
pub(crate) fn fast_vault_options(
    clock: &ManualClock,
    secret_columns: &'static [SecretColumn],
) -> VaultOptions {
    VaultOptions {
        clock: Arc::new(clock.clone()),
        kdf: KdfParams {
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        },
        failure_delay: Duration::ZERO,
        secret_columns,
    }
}

/// A vault over `db` with [`fast_vault_options`] for every secret column.
pub(crate) async fn fast_vault(db: Db) -> Vault {
    let options = fast_vault_options(&ManualClock::new(), SECRET_COLUMNS);
    Vault::open_with(db, options).await.expect("vault")
}

/// An engine on the database file in `dir`, with a vault that derives in milliseconds.
pub(crate) async fn engine_in(dir: &Path) -> Engine {
    let db = Db::open(dir.join(DB_FILE)).await.expect("db");
    let engine = headless(dir).await;
    engine.attach_db(db.clone());
    assert!(engine.attach_vault(fast_vault(db).await));
    engine
}

/// The panel `command` answers.
pub(crate) async fn run_panel(engine: &Engine, command: Command) -> PanelSnapshot {
    expect_panel(engine.dispatch(command).await.expect("runs"))
}

pub(crate) fn expect_panel(outcome: Outcome) -> PanelSnapshot {
    match outcome {
        Outcome::Panel(snapshot) => snapshot,
        other => panic!("a panel, got {other:?}"),
    }
}

pub(crate) fn expect_drives(outcome: Outcome) -> Vec<Drive> {
    match outcome {
        Outcome::Drives { drives } => drives,
        other => panic!("drives, got {other:?}"),
    }
}

pub(crate) fn expect_job(outcome: Outcome) -> JobSnapshot {
    match outcome {
        Outcome::Job(job) => *job,
        other => panic!("a job, got {other:?}"),
    }
}

/// The jobs of an outcome that queued one or several.
pub(crate) fn expect_jobs(outcome: Outcome) -> Vec<JobSnapshot> {
    match outcome {
        Outcome::Job(job) => vec![*job],
        Outcome::Jobs { jobs } => jobs,
        other => panic!("jobs, got {other:?}"),
    }
}

pub(crate) fn expect_tabs(outcome: Outcome) -> Vec<TabView> {
    match outcome {
        Outcome::Tabs { tabs, .. } => tabs,
        other => panic!("tabs, got {other:?}"),
    }
}

#[cfg(unix)]
pub(crate) fn expect_terminal(outcome: Outcome) -> crate::TerminalInfo {
    match outcome {
        Outcome::Terminal(info) => info,
        other => panic!("a terminal, got {other:?}"),
    }
}

/// A secret of `kind` stored with the connection `id`, as the manager's form stores it: a
/// `connections.save` of nothing but that secret.
pub(crate) fn store_secret(id: ConnectionId, kind: SecretKind, value: SecretParam) -> Command {
    save_secret(id, SecretChange::Set { kind, value })
}

/// The stored secret of `kind` removed from the connection `id`, as the manager's form removes it.
pub(crate) fn clear_secret(id: ConnectionId, kind: SecretKind) -> Command {
    save_secret(id, SecretChange::Clear { kind })
}

fn save_secret(id: ConnectionId, change: SecretChange) -> Command {
    Command::SaveConnection(SaveConnectionRequest {
        fields: SaveFields::Update {
            id,
            patch: ConnectionPatch::default(),
        },
        secrets: vec![change],
        login_script: None,
        favourite: None,
        credential: None,
    })
}
