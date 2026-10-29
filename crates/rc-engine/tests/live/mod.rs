//! The harness of the live SFTP tests (RC-080-14): real OpenSSH servers in Docker, started by
//! `scripts/live/sftp/run.sh`, which also sets the environment read here. Without it every test
//! says so and passes without doing anything; the tests are `#[ignore]` besides, so a plain
//! `cargo nextest run` never reaches a server.
//!
//! Each test has an engine of its own, with a database file and a vault in a folder below the
//! build's temporary directory (on disk: `/tmp` may be memory), and its own folder on a server.
//! A server a test disturbs - a new host key, dropped sessions, a stop - is that test's alone.

use std::{
    path::{Path, PathBuf},
    process::Command as Process,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use rc_db::{Db, SECRET_COLUMNS};
use rc_engine::{
    Command, ConnectionDraft, ConnectionId, ConnectionPatch, Engine, EngineError, HostKeyAnswer,
    JobId, JobOptions, JobSnapshot, JobState, Outcome, PanelId, SaveConnectionRequest, SaveFields,
    SecretChange, SecretKind, SecretParam, TransferRequest,
};
use rc_vault::{KdfParams, ManualClock, Vault, VaultOptions};
use rc_vfs::VfsPath;

/// The master password of every test's vault (a dummy).
pub const MASTER: &str = "live test master password";

/// The servers, in the order of their ports from `RC_LIVE_SFTP_PORT_BASE`; the same table as
/// `scripts/live/sftp/ctl.sh`.
const ROLES: [&str; 13] = [
    "a",
    "b",
    "keys",
    "drop",
    "spent",
    "restart",
    "hang",
    "alive",
    "faults-list",
    "faults-copy",
    "faults-term",
    "many",
    "lost",
];

/// What `run.sh` exports.
pub struct Live {
    pub host: String,
    pub user: String,
    pub password: String,
    ctl: PathBuf,
    base: u16,
    /// The size of the random file `/home/tester/big.bin` on every server.
    pub big: u64,
}

/// The environment, or `None` (the test then returns) when the servers are not running.
pub fn live() -> Option<Live> {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let Some(ctl) = var("RC_LIVE_SFTP_CTL") else {
        eprintln!("skipped: RC_LIVE_SFTP_CTL is not set (scripts/live/sftp/run.sh sets it)");
        return None;
    };
    let number = |name: &str, default: u64| {
        var(name).map_or(default, |value| value.parse().expect("a number"))
    };
    Some(Live {
        host: var("RC_LIVE_SFTP_HOST").unwrap_or_else(|| "127.0.0.1".to_owned()),
        user: var("RC_LIVE_SFTP_USER").unwrap_or_else(|| "tester".to_owned()),
        password: var("RC_LIVE_SFTP_PASSWORD").expect("RC_LIVE_SFTP_PASSWORD"),
        ctl: PathBuf::from(ctl),
        base: u16::try_from(number("RC_LIVE_SFTP_PORT_BASE", 2201)).expect("a port"),
        big: number("RC_LIVE_SFTP_BIG_MB", 300) * 1024 * 1024,
    })
}

/// The environment, or the end of the test.
#[macro_export]
macro_rules! live_or_skip {
    () => {
        match $crate::live::live() {
            Some(live) => live,
            None => return,
        }
    };
}

impl Live {
    pub fn port(&self, role: &str) -> u16 {
        let index = ROLES
            .iter()
            .position(|known| *known == role)
            .expect("a role of ctl.sh");
        self.base + u16::try_from(index).expect("few roles")
    }

    /// Runs `ctl.sh` with `args`; its standard output.
    pub fn ctl(&self, args: &[&str]) -> String {
        let output = Process::new(&self.ctl)
            .args(args)
            .output()
            .expect("ctl.sh runs");
        assert!(
            output.status.success(),
            "ctl.sh {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("text")
    }

    /// The SHA-256 of a file on a server, by the server's own `sha256sum`.
    pub fn remote_sha256(&self, role: &str, path: &str) -> String {
        self.ctl(&["sha256", role, path]).trim().to_owned()
    }

    /// Whether `path` exists on the server.
    pub fn remote_exists(&self, role: &str, path: &str) -> bool {
        self.ctl(&[
            "exec",
            role,
            &format!("test -e '{path}' && echo yes || echo no"),
        ])
        .trim()
            == "yes"
    }

    /// The names in a folder on the server.
    pub fn remote_names(&self, role: &str, dir: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .ctl(&["exec", role, &format!("ls -A '{dir}'")])
            .lines()
            .map(str::to_owned)
            .collect();
        names.sort();
        names
    }

    /// A folder on the server that only this test uses, owned by the user.
    pub fn remote_dir(&self, role: &str, name: &str) -> String {
        let dir = format!("/home/tester/{name}");
        self.ctl(&[
            "exec",
            role,
            &format!("rm -rf '{dir}' && mkdir -p '{dir}' && chown tester:tester '{dir}'"),
        ]);
        dir
    }

    /// How many `userauth-request` lines the server logged: a login attempt of any method.
    pub fn userauth_requests(&self, role: &str) -> usize {
        self.ctl(&["logs", role])
            .matches("userauth-request")
            .count()
    }

    /// How many logins the server accepted.
    pub fn logins(&self, role: &str) -> usize {
        self.ctl(&["logs", role])
            .matches("Accepted password")
            .count()
    }

    /// The `SHA256:` fingerprints of the server's host keys.
    pub fn fingerprints(&self, role: &str) -> Vec<String> {
        self.ctl(&["fingerprints", role])
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// The SHA-256 of a local file, by the system's `sha256sum` (not rCommander's own code).
pub fn local_sha256(path: &Path) -> String {
    let output = Process::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum runs");
    assert!(output.status.success(), "sha256sum {}", path.display());
    String::from_utf8(output.stdout)
        .expect("text")
        .split_whitespace()
        .next()
        .expect("a digest")
        .to_owned()
}

/// A folder below the build's temporary directory, which is on disk.
pub fn workdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work folder")
}

/// A file of `size` pseudo-random bytes (xorshift; incompressible enough for a transfer).
pub fn random_file(path: &Path, size: u64, seed: u64) {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).expect("create"));
    let mut state = seed | 1;
    let mut block = vec![0_u8; 1024 * 1024];
    let mut left = size;
    while left > 0 {
        for chunk in block.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            chunk.copy_from_slice(&state.to_le_bytes()[..chunk.len()]);
        }
        let take = usize::try_from(left.min(block.len() as u64)).expect("fits");
        file.write_all(&block[..take]).expect("write");
        left -= take as u64;
    }
    file.flush().expect("flush");
}

/// The modification time of a local file, in whole seconds since 1970.
pub fn local_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .expect("a time")
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs()
}

/// An engine whose two panels show `dir`, with the database `dir/rcommander.db` and an unlocked
/// vault (created on the first start, unlocked on a later one). The database is returned for a
/// test that starts the engine again.
pub async fn engine(dir: &Path) -> (Engine, Db) {
    // `RUST_LOG=rc_engine=debug,rc_ops=debug,rc_remote=debug` traces a run that hangs.
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
    }
    let db = Db::open(dir.join("rcommander.db")).await.expect("db");
    let engine = Engine::open(VfsPath::local(dir), VfsPath::local(dir))
        .await
        .expect("engine");
    assert!(engine.attach_db(db.clone()));
    engine.attach_agent_sources(Vec::new());
    let vault = Vault::open_with(
        db.clone(),
        VaultOptions {
            clock: Arc::new(ManualClock::new()),
            kdf: KdfParams {
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            },
            failure_delay: Duration::ZERO,
            secret_columns: SECRET_COLUMNS,
        },
    )
    .await
    .expect("vault");
    let exists = vault.status().state != rc_engine::VaultStateKind::Absent;
    engine.attach_vault(vault);
    let command = if exists {
        Command::UnlockVault {
            password: SecretParam::from(MASTER),
        }
    } else {
        Command::CreateVault {
            password: SecretParam::from(MASTER),
        }
    };
    engine.dispatch(command).await.expect("the vault opens");
    (engine, db)
}

/// The saved SSH connection `name` to the server of `role`, its password in the vault. Its host
/// key is not trusted yet.
pub async fn connection(engine: &Engine, live: &Live, name: &str, role: &str) -> ConnectionId {
    let draft = ConnectionDraft {
        port: Some(i64::from(live.port(role))),
        username: Some(live.user.clone()),
        ..ConnectionDraft::new(name, live.host.clone())
    };
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    engine
        .dispatch(Command::SaveConnection(SaveConnectionRequest {
            fields: SaveFields::Update {
                id: connection.id,
                patch: ConnectionPatch::default(),
            },
            secrets: vec![SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(live.password.as_str()),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    connection.id
}

/// Answers the open question about an unknown key with "trust and save".
pub async fn trust_open_question(engine: &Engine) {
    let question = engine
        .host_key_questions()
        .pop()
        .expect("a host key question");
    engine
        .dispatch(Command::AnswerHostKey {
            question: question.id,
            answer: HostKeyAnswer::TrustAndSave,
        })
        .await
        .expect("trusted");
}

/// Connects `id` the way the window does the first time: the unknown key is asked about,
/// trusted and saved, and the connection tried again.
pub async fn connect_trusting(engine: &Engine, id: ConnectionId) {
    let first = engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect_err("an unknown key first");
    assert_eq!(first.code(), "host_key_unknown", "{first}");
    trust_open_question(engine).await;
    engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect("connected");
}

/// `sftp://<connection>/<path>`.
pub fn sftp(connection: &str, path: &str) -> String {
    format!("sftp://{connection}{path}")
}

fn job_of(outcome: Result<Outcome, EngineError>) -> JobId {
    match outcome {
        Ok(Outcome::Job(job)) => job.id,
        other => panic!("a job, got {other:?}"),
    }
}

/// Queues a copy (`moving`: a move) of `sources` into `target`. A target is read against the
/// other panel's backend, as the F5 dialog's is: a local target is sent from the right panel,
/// so that the other one is the left, which every test keeps on the local disk.
pub async fn transfer(
    engine: &Engine,
    sources: Vec<String>,
    target: String,
    options: JobOptions,
    moving: bool,
) -> JobId {
    let panel = if target.contains("://") {
        PanelId::Left
    } else {
        PanelId::Right
    };
    let request = TransferRequest {
        panel: Some(panel),
        sources,
        target: Some(target),
        options,
    };
    let command = if moving {
        Command::OpsMove(request)
    } else {
        Command::OpsCopy(request)
    };
    job_of(engine.dispatch(command).await)
}

/// The job once it is over (completed, cancelled or failed), `limit` at most.
pub async fn finished(engine: &Engine, job: JobId, limit: Duration) -> JobSnapshot {
    until(engine, job, limit, "over", |snapshot| {
        snapshot.state.is_finished()
    })
    .await
}

/// Waits until `check` holds for the job, `limit` at most; the snapshot that held.
pub async fn until(
    engine: &Engine,
    job: JobId,
    limit: Duration,
    what: &str,
    check: impl Fn(&JobSnapshot) -> bool,
) -> JobSnapshot {
    let started = Instant::now();
    loop {
        let snapshot = engine.ops().job(job).expect("a known job");
        if check(&snapshot) {
            return snapshot;
        }
        assert!(
            started.elapsed() < limit,
            "never {what}: {:?} {:?} {:?} {:?} {:?}",
            snapshot.state,
            snapshot.kind,
            snapshot.question,
            snapshot.progress,
            snapshot.failures
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Waits until the job has moved between 5 and 60 percent of `total` bytes.
pub async fn under_way(engine: &Engine, job: JobId, total: u64) -> JobSnapshot {
    until(
        engine,
        job,
        Duration::from_secs(60),
        "under way",
        |snapshot| {
            let done = snapshot.progress.bytes_done;
            done > total / 20 && done < total / 100 * 60
        },
    )
    .await
}

/// Asserts a job completed without a failure.
pub fn completed(job: &JobSnapshot) {
    assert_eq!(job.state, JobState::Completed, "{:?}", job.failures);
    assert!(job.failures.is_empty(), "{:?}", job.failures);
}
