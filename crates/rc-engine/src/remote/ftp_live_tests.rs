//! FTP and FTPS against real servers (vsftpd, pure-ftpd, ProFTPD in containers), through the
//! engine as the window drives it. Opt-in: every test is `#[ignore]`d and reads the server from
//! `RC_LIVE_FTP_*`; `scripts/live/ftp/run.sh` starts the servers, runs these tests in the
//! servers' network namespace and removes the containers again. A normal test run never needs
//! Docker.
//!
//! `live_ftp_core_*` run for every server, encryption and data mode; `live_ftp_tls_*` for the
//! encrypted ones; the operations in `ftp_live_ops_tests.rs` once per server.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_ops::{ConflictChoice, JobOptions, JobSnapshot, JobState};
use rc_vfs::{Vfs, VfsPath};

use super::*;
use crate::{
    CertificateAnswer, CertificateQuestionKind, Command, ConnectionDraft, Outcome, PanelId,
    PanelSnapshot, SecretParam, TransferRequest,
    remote::sftp::tests::{Harness, harness},
};

/// The connection's name in `ftp://live/...` locations.
pub(super) const NAME: &str = "live";

/// The server under test, from the environment `scripts/live/ftp/run.sh` sets.
pub(super) struct Env {
    pub port: u16,
    pub kind: ConnectionKind,
    pub mode: FtpDataMode,
    pub user: String,
    pub password: String,
    /// Where the login starts (`PWD`).
    pub home: String,
}

pub(super) fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: run scripts/live/ftp/run.sh"))
}

/// An optional port of the environment, such as the slow instance's.
pub(super) fn port_var(name: &str) -> Option<u16> {
    std::env::var(name)
        .ok()
        .filter(|port| !port.is_empty())
        .map(|port| port.parse().expect("a port"))
}

pub(super) fn env() -> Env {
    Env {
        port: var("RC_LIVE_FTP_PORT").parse().expect("a port"),
        kind: match var("RC_LIVE_FTP_KIND").as_str() {
            "ftp" => ConnectionKind::Ftp,
            "ftps" => ConnectionKind::Ftps,
            "ftps_implicit" => ConnectionKind::FtpsImplicit,
            other => panic!("unknown RC_LIVE_FTP_KIND {other}"),
        },
        mode: match var("RC_LIVE_FTP_MODE").as_str() {
            "passive" => FtpDataMode::Passive,
            "active" => FtpDataMode::Active,
            other => panic!("unknown RC_LIVE_FTP_MODE {other}"),
        },
        user: var("RC_LIVE_FTP_USER"),
        password: var("RC_LIVE_FTP_PASSWORD"),
        home: var("RC_LIVE_FTP_HOME"),
    }
}

/// An engine with the connection `live` to the server; its certificate trusted when `trust`.
pub(super) struct Live {
    pub base: Harness,
    pub id: ConnectionId,
    pub env: Env,
}

impl Live {
    pub fn engine(&self) -> &Engine {
        &self.base.engine
    }

    /// The local directory of the left panel.
    pub fn local(&self) -> PathBuf {
        self.engine().panel(PanelId::Left).path.path
    }

    /// `path` on the server as a location.
    pub fn location(&self, path: &str) -> String {
        format!("ftp://{NAME}{path}")
    }

    /// The kept session as a file system.
    pub async fn vfs(&self) -> Arc<dyn Vfs> {
        self.engine()
            .open_vfs(
                &VfsPath {
                    backend: backend(self.id),
                    path: "/".into(),
                },
                false,
            )
            .await
            .expect("the session")
    }
}

/// The connection to the environment's server on `port` (else the environment's), opening in
/// `start` when given.
pub(super) async fn live_on(port: Option<u16>, start: Option<&str>, trust: bool) -> Live {
    let env = env();
    let base = harness().await;
    let engine = &base.engine;
    let draft = ConnectionDraft {
        kind: env.kind,
        port: Some(i64::from(port.unwrap_or(env.port))),
        username: Some(env.user.clone()),
        auth_method: AuthMethod::Password,
        ftp_data_mode: Some(env.mode),
        start_directory: start.map(str::to_owned),
        ..ConnectionDraft::new(NAME, "127.0.0.1")
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
        .dispatch(Command::SaveConnection(crate::SaveConnectionRequest {
            fields: crate::SaveFields::Update {
                id: connection.id,
                patch: crate::ConnectionPatch::default(),
            },
            secrets: vec![crate::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(env.password.as_str()),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    let live = Live {
        base,
        id: connection.id,
        env,
    };
    if trust && live.env.kind != ConnectionKind::Ftp {
        let refused = live.test().await.expect_err("a self-signed certificate");
        assert_eq!(refused.code(), "certificate_unknown");
        answer(live.engine(), CertificateAnswer::TrustAndSave).await;
    }
    live
}

pub(super) async fn live(trust: bool) -> Live {
    live_on(None, None, trust).await
}

impl Live {
    /// "Test" in the connection manager.
    pub async fn test(&self) -> Result<Outcome, EngineError> {
        self.engine()
            .dispatch(Command::TestConnection { id: self.id })
            .await
    }

    /// A click on the server's drive button in `panel`.
    pub async fn open(&self, panel: PanelId) -> Result<PanelSnapshot, EngineError> {
        let outcome = self
            .engine()
            .dispatch(Command::ChangeDrive {
                panel: Some(panel),
                drive: format!("server:{}", self.id),
            })
            .await?;
        let Outcome::Panel(snapshot) = outcome else {
            panic!("a panel, got {outcome:?}");
        };
        Ok(snapshot)
    }

    /// F5 (or F6 when `moving`) of `sources` in `panel` into `target`, overwriting; the job once
    /// it ended.
    pub async fn transfer(
        &self,
        panel: PanelId,
        sources: &[&str],
        target: &str,
        moving: bool,
    ) -> JobSnapshot {
        let job = self.submit(panel, sources, target, moving).await;
        self.engine().ops().wait(job).await.expect("known")
    }

    /// As [`Live::transfer`], without waiting.
    pub async fn submit(
        &self,
        panel: PanelId,
        sources: &[&str],
        target: &str,
        moving: bool,
    ) -> rc_ops::JobId {
        let request = TransferRequest {
            panel: Some(panel),
            sources: sources.iter().map(|source| (*source).to_owned()).collect(),
            target: Some(target.to_owned()),
            options: JobOptions {
                on_conflict: Some(ConflictChoice::Overwrite),
                ..JobOptions::default()
            },
        };
        let command = if moving {
            Command::OpsMove(request)
        } else {
            Command::OpsCopy(request)
        };
        let outcome = self.engine().dispatch(command).await.expect("queued");
        let Outcome::Job(job) = outcome else {
            panic!("a job, got {outcome:?}");
        };
        job.id
    }
}

/// How many `PASS` commands the server logged on `port`, where it logs commands (vsftpd's
/// `log_ftp_protocol`, read through `RC_LIVE_FTP_LOGS`).
pub(super) fn passwords_logged(port: u16) -> Option<usize> {
    let logs = std::env::var("RC_LIVE_FTP_LOGS").ok()?;
    let text = std::fs::read_to_string(format!("{logs}/vsftpd-{port}.log")).ok()?;
    Some(text.lines().filter(|line| line.contains("\"PASS ")).count())
}

/// Answers the oldest open certificate question with `answer`.
pub(super) async fn answer(engine: &Engine, answer: CertificateAnswer) {
    let question = engine
        .certificate_questions()
        .into_iter()
        .next()
        .expect("a certificate question");
    engine
        .dispatch(Command::AnswerCertificate {
            question: question.id,
            answer,
        })
        .await
        .expect("answered");
}

/// `len` bytes that no compression or dedup flatters.
pub(super) fn content(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

pub(super) fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Asserts `job` completed without failures.
pub(super) fn completed(job: &JobSnapshot) {
    assert_eq!(job.state, JobState::Completed, "{job:?}");
    assert!(job.failures.is_empty(), "{job:?}");
}

/// The bytes of `path` on the server.
pub(super) async fn read_remote(vfs: &Arc<dyn Vfs>, path: &str) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    vfs.open_read(Path::new(path))
        .await
        .expect("readable")
        .read_to_end(&mut bytes)
        .await
        .expect("read");
    bytes
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_core_test_and_open_at_login_with_rights() {
    let live = live(true).await;
    live.test().await.expect("Test passes");
    let panel = live.open(PanelId::Right).await.expect("the panel opens");
    assert_eq!(
        panel.path.path,
        Path::new(&live.env.home),
        "where the login starts"
    );
    let entries = live
        .engine()
        .dispatch(Command::ListDir {
            panel: Some(PanelId::Right),
            path: None,
        })
        .await
        .expect("listed");
    let Outcome::Listing { entries, .. } = entries else {
        panic!("a listing");
    };
    let docs = entries
        .iter()
        .find(|entry| entry.name == "docs")
        .expect("docs listed");
    assert!(docs.is_dir_like(), "{docs:?}");
    assert!(
        docs.attributes.mode.is_some(),
        "the Rights column: {docs:?}"
    );
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_core_opens_in_the_start_folder() {
    let start = format!("{}/start", env().home);
    let live = live_on(None, Some(&start), true).await;
    let panel = live.open(PanelId::Right).await.expect("the panel opens");
    assert_eq!(panel.path.path, Path::new(&start));
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_core_upload_list_download() {
    let live = live(true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    let name = format!("small-{}.txt", std::process::id());
    std::fs::write(live.local().join(&name), b"live upload").expect("local file");
    let home = live.env.home.clone();
    completed(
        &live
            .transfer(PanelId::Left, &[&name], &live.location(&home), false)
            .await,
    );
    let listed = live
        .vfs()
        .await
        .list(Path::new(&home))
        .await
        .expect("listed");
    assert!(
        listed.iter().any(|entry| entry.name == name),
        "uploaded file listed"
    );

    let back = live.local().join("back");
    std::fs::create_dir_all(&back).expect("folder");
    let target = back.display().to_string();
    completed(
        &live
            .transfer(PanelId::Right, &[&name], &target, false)
            .await,
    );
    assert_eq!(
        std::fs::read(back.join(&name)).expect("downloaded"),
        b"live upload"
    );
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_core_big_file_byte_for_byte_both_ways() {
    let live = live(true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    let data = content(48 * 1024 * 1024 + 4321);
    let name = format!("big-{}.bin", std::process::id());
    std::fs::write(live.local().join(&name), &data).expect("local file");
    let home = live.env.home.clone();
    completed(
        &live
            .transfer(PanelId::Left, &[&name], &live.location(&home), false)
            .await,
    );
    let uploaded = read_remote(&live.vfs().await, &format!("{home}/{name}")).await;
    assert_eq!(digest(&uploaded), digest(&data), "local to FTP");

    let back = live.local().join("back");
    std::fs::create_dir_all(&back).expect("folder");
    completed(
        &live
            .transfer(PanelId::Right, &[&name], &back.display().to_string(), false)
            .await,
    );
    let downloaded = std::fs::read(back.join(&name)).expect("downloaded");
    assert!(downloaded == data, "FTP to local, byte for byte");
    println!(
        "big file: {} bytes, digest {:016x}",
        data.len(),
        digest(&data)
    );
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_tls_unknown_certificate_accept_once_then_trust() {
    let live = live(false).await;
    let passwords = passwords_logged(live.env.port);
    let refused = live.test().await.expect_err("self-signed");
    assert_eq!(refused.code(), "certificate_unknown");
    match (passwords, passwords_logged(live.env.port)) {
        (Some(before), Some(after)) => {
            assert_eq!(before, after, "no PASS before the certificate is decided");
            println!("server log: no PASS reached the server before the decision");
        }
        _ => println!("this server logs no commands: PASS count not checked"),
    }
    answer(live.engine(), CertificateAnswer::AcceptOnce).await;
    live.open(PanelId::Right).await.expect("accepted once");
    live.engine()
        .dispatch(Command::DisconnectSession { id: live.id })
        .await
        .expect("disconnected");
    let again = live.test().await.expect_err("once is spent");
    assert_eq!(again.code(), "certificate_unknown");
    answer(live.engine(), CertificateAnswer::TrustAndSave).await;
    live.test().await.expect("trusted from now on");
    live.open(PanelId::Right).await.expect("trusted");
}

/// The server's certificate replaced and the instance restarted (`RC_LIVE_FTP_RECERT`, run in
/// the server's namespaces) after it was trusted: refused as `certificate_changed`, a hard stop.
#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_tls_changed_certificate_is_refused() {
    let live = live(true).await;
    live.test().await.expect("trusted");
    let recert = var("RC_LIVE_FTP_RECERT");
    // console-window: allow (a test helper on Linux, in the test's container)
    let status = std::process::Command::new(&recert)
        .arg(live.env.port.to_string())
        .status()
        .expect("the recert helper runs");
    assert!(status.success(), "{recert} failed");
    let refused = live.test().await.expect_err("changed");
    assert_eq!(refused.code(), "certificate_changed");
    let question = live.engine().certificate_questions();
    assert!(
        matches!(
            question.first().map(|question| &question.kind),
            Some(CertificateQuestionKind::Changed { .. })
        ),
        "{question:?}"
    );
}
