//! "Install and restart" without the download (RC-100-84), against a fetcher in memory and a
//! portable test installation whose hand-overs are counted instead of starting the updater: the
//! click downloads and installs by itself, a busy queue holds it back, a failed download drops
//! it, and quitting never waits for a running download.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use rc_update::{MemoryFetcher, Sources, fetch};
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::{Semaphore, broadcast};

use crate::updates::DownloadState;
use crate::updates::tests::{served_with, signed_for, status_of};
use crate::{Command, Engine, EngineEvent, Outcome, settings};

const VERSION: &str = "99.0.0";
const ARCHIVE: &[u8] = b"the next version's archive";

fn artifact_url() -> String {
    format!(
        "https://github.com/degoya/rCommander/releases/download/v{VERSION}/rcommander-{VERSION}-test.zip"
    )
}

/// A fetcher in memory whose downloads each take a permit for good: the test decides when, and
/// how many, they go ahead.
struct Gated {
    inner: MemoryFetcher,
    gate: Arc<Semaphore>,
}

#[async_trait::async_trait]
impl rc_update::Fetcher for Gated {
    async fn fetch(&self, url: &fetch::Url, limit: u64) -> anyhow::Result<Option<Vec<u8>>> {
        rc_update::Fetcher::fetch(&self.inner, url, limit).await
    }

    async fn open(&self, url: &fetch::Url) -> anyhow::Result<fetch::Download> {
        self.gate.acquire().await?.forget();
        rc_update::Fetcher::open(&self.inner, url).await
    }
}

struct Setup {
    _root: TempDir,
    engine: Engine,
    launches: Arc<AtomicUsize>,
    gate: Arc<Semaphore>,
}

/// An engine offered [`VERSION`] (the archive served unless `archive` is false), a portable
/// installation, `update.auto_install` as `auto`, and downloads behind a gate with `permits`.
async fn offered(archive: bool, auto: Option<&str>, permits: usize) -> Setup {
    let fetcher = MemoryFetcher::new();
    let manifest = signed_for(
        VERSION,
        1,
        &rc_update::sign::hex_sha256(ARCHIVE),
        u64::try_from(ARCHIVE.len()).expect("small"),
    );
    let sources = Sources::official().expect("sources");
    fetcher.serve(sources.stable.as_str(), manifest);
    if archive {
        fetcher.serve(&artifact_url(), ARCHIVE);
    }
    let gate = Arc::new(Semaphore::new(permits));
    let gated = Gated {
        inner: fetcher,
        gate: Arc::clone(&gate),
    };
    let (root, engine) = served_with(Arc::new(gated)).await;
    let install = root.path().join("app");
    std::fs::create_dir_all(&install).expect("program folder");
    let launches = engine.use_portable_installation(root.path().join("data"), install);
    if let Some(auto) = auto {
        engine
            .set_protected_setting(settings::UPDATE_AUTO_KEY, &json!(auto))
            .await
            .expect("auto install");
    }
    let status = status_of(engine.dispatch(Command::CheckForUpdates {}).await);
    assert_eq!(status.offer.expect("offered").version, VERSION);
    Setup {
        _root: root,
        engine,
        launches,
        gate,
    }
}

/// Waits until `done` holds for the status, or fails after five seconds.
async fn until(engine: &Engine, what: &str, done: impl Fn(&crate::UpdateStatus) -> bool) {
    for _ in 0..500 {
        if done(&engine.update_status().await) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} did not happen: {:?}", engine.update_status().await);
}

/// Whether the engine asks the window to end within `wait`.
async fn quits(events: &mut broadcast::Receiver<EngineEvent>, wait: Duration) -> bool {
    tokio::time::timeout(wait, async {
        loop {
            match events.recv().await {
                Ok(EngineEvent::Quit { .. }) => return true,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

#[tokio::test]
async fn install_without_the_download_downloads_and_installs_by_itself() {
    let setup = offered(true, Some("off"), 0).await;
    let mut events = setup.engine.subscribe();
    let status = status_of(setup.engine.dispatch(Command::InstallUpdate {}).await);
    assert!(status.install_after_download, "{status:?}");
    assert!(
        matches!(status.download, DownloadState::Running { .. }),
        "{status:?}"
    );
    assert_eq!(setup.launches.load(Ordering::SeqCst), 0);
    setup.gate.add_permits(1);
    assert!(quits(&mut events, Duration::from_secs(10)).await, "no quit");
    assert_eq!(setup.launches.load(Ordering::SeqCst), 1);
    let status = setup.engine.update_status().await;
    assert!(status.installing, "{status:?}");
    assert!(!status.install_after_download, "{status:?}");
}

/// A job in the queue when the download is ready holds the install back until the queue is empty.
#[tokio::test]
async fn a_requested_install_waits_for_the_empty_queue() {
    let setup = offered(true, Some("off"), 0).await;
    let engine = &setup.engine;
    let mut events = engine.subscribe();
    status_of(engine.dispatch(Command::InstallUpdate {}).await);
    let outcome = engine
        .dispatch(Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec!["a.txt".into()],
            target: Some("docs".into()),
            options: rc_ops::JobOptions::default(),
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = outcome else {
        panic!("a job, got {outcome:?}");
    };
    // The copy waits for the answer to its conflict about `docs/a.txt`: unfinished.
    for _ in 0..500 {
        if engine.ops().job(job.id).expect("known").question.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(engine.jobs_unfinished());
    setup.gate.add_permits(1);
    until(engine, "the download", |status| {
        matches!(status.download, DownloadState::Ready { .. })
    })
    .await;
    assert!(!quits(&mut events, Duration::from_millis(2500)).await);
    assert_eq!(setup.launches.load(Ordering::SeqCst), 0);
    let status = engine.update_status().await;
    assert!(
        status.install_after_download && status.queue_busy,
        "{status:?}"
    );
    engine
        .dispatch(Command::OpsCancel { job: job.id })
        .await
        .expect("cancelled");
    assert!(quits(&mut events, Duration::from_secs(10)).await, "no quit");
    assert_eq!(setup.launches.load(Ordering::SeqCst), 1);
}

/// A failed download drops the request: no install, and the click can be made again.
#[tokio::test]
async fn a_failed_download_installs_nothing() {
    let setup = offered(false, Some("off"), 1).await;
    let mut events = setup.engine.subscribe();
    status_of(setup.engine.dispatch(Command::InstallUpdate {}).await);
    until(&setup.engine, "the failure", |status| {
        matches!(status.download, DownloadState::Failed { .. })
    })
    .await;
    let status = setup.engine.update_status().await;
    assert!(!status.install_after_download, "{status:?}");
    assert!(!status.installing, "{status:?}");
    assert!(!quits(&mut events, Duration::from_millis(500)).await);
    assert_eq!(setup.launches.load(Ordering::SeqCst), 0);
    // Asked again, it downloads again - held at the gate, whose one permit the first took.
    let status = status_of(setup.engine.dispatch(Command::InstallUpdate {}).await);
    assert!(status.install_after_download, "{status:?}");
    assert!(
        matches!(status.download, DownloadState::Running { .. }),
        "{status:?}"
    );
}

/// Quitting with the default `on_quit` while the download runs: the window ends at once and
/// nothing is installed, the click before it notwithstanding.
#[tokio::test]
async fn quitting_never_waits_for_a_running_download() {
    let setup = offered(true, None, 0).await;
    // The check of the default `on_quit` started the download already; the click joins it.
    let status = status_of(setup.engine.dispatch(Command::InstallUpdate {}).await);
    assert!(
        matches!(status.download, DownloadState::Running { .. }),
        "{status:?}"
    );
    let handed = tokio::time::timeout(Duration::from_secs(1), setup.engine.update_on_quit())
        .await
        .expect("quitting does not wait");
    assert!(!handed);
    assert_eq!(setup.launches.load(Ordering::SeqCst), 0);
}

/// Only a portable installation installs: without one the click is refused and nothing
/// downloads.
#[tokio::test]
async fn an_installation_that_does_not_update_itself_is_refused() {
    let (_root, engine) = served_with(Arc::new(MemoryFetcher::new())).await;
    let refused = engine
        .dispatch(Command::InstallUpdate {})
        .await
        .expect_err("not started");
    assert_eq!(refused.code(), "update.not_started");
    let status = engine.update_status().await;
    assert!(!status.install_after_download);
    assert_eq!(status.download, DownloadState::None);
}
