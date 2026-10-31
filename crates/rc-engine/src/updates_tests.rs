//! The self-update in the engine, against a fetcher in memory and a throw-away key (RC-100-05):
//! what a check offers and stores, what the settings switch, what MCP may not do, and that a
//! hanging host holds nothing up. No network.

use std::{sync::Arc, time::Duration};

use chrono::{Duration as Days, Utc};
use rc_db::Db;
use rc_update::{
    Artifact, Channel, MemoryFetcher, Sources, UpdateManifest,
    manifest::{self, UPDATE_MANIFEST_SCHEMA_VERSION},
    sign::{SigningKey, TrustStore, UPDATE_KEY_ID},
};
use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::{Command, EngineError, Outcome, settings, test_support};

fn key() -> SigningKey {
    SigningKey::from_bytes(&[3; 32])
}

fn trust() -> TrustStore {
    let trust = TrustStore::new();
    trust
        .trust(UPDATE_KEY_ID.to_owned(), key().verifying_key())
        .expect("trust");
    trust
}

fn signed(version: &str, sequence: u64) -> Vec<u8> {
    let now = Utc::now();
    let target = rc_update::Target::current();
    let manifest = UpdateManifest {
        schema_version: UPDATE_MANIFEST_SCHEMA_VERSION,
        sequence,
        issued_at: now,
        not_after: now + Days::days(30),
        channel: Channel::Stable,
        version: version.to_owned(),
        released_at: now,
        notes: "- Faster.".to_owned(),
        artifacts: vec![Artifact {
            platform: target.platform.to_owned(),
            arch: target.arch.to_owned(),
            kind: target.kind.to_owned(),
            url: format!(
                "https://github.com/degoya/rCommander/releases/download/v{version}/rcommander-{version}-test.zip"
            ),
            sha256: "ab".repeat(32),
            size: 10,
        }],
        schema_change: Some(false),
    };
    manifest::sign(UPDATE_KEY_ID, &key(), &manifest).expect("sign")
}

/// An engine with a database file and `fetcher` in place of the release host.
async fn served_with(fetcher: Arc<dyn rc_update::Fetcher>) -> (TempDir, Engine) {
    let root = test_support::tree(&[("a.txt", "a"), ("docs/a.txt", "old")]);
    let engine = test_support::headless(root.path()).await;
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).expect("data");
    let db = Db::open(data.join("rcommander.db")).await.expect("db");
    assert!(engine.attach_db(db));
    engine.use_update_sources(fetcher, trust());
    (root, engine)
}

/// [`served_with`] a fetcher in memory serving `stable` at the official address.
async fn served(stable: Option<Vec<u8>>) -> (TempDir, Engine, MemoryFetcher) {
    let fetcher = MemoryFetcher::new();
    if let Some(body) = stable {
        let sources = Sources::official().expect("sources");
        fetcher.serve(sources.stable.as_str(), body);
    }
    let (root, engine) = served_with(Arc::new(fetcher.clone())).await;
    (root, engine, fetcher)
}

fn status_of(outcome: Result<Outcome, EngineError>) -> UpdateStatus {
    match outcome {
        Ok(Outcome::Update(status)) => *status,
        other => panic!("expected the update status, got {other:?}"),
    }
}

#[tokio::test]
async fn a_check_offers_a_higher_version_and_stores_its_floor() {
    let (_root, engine, fetcher) = served(Some(signed("99.0.0", 42))).await;
    let status = status_of(engine.dispatch(Command::CheckForUpdates {}).await);
    assert_eq!(status.last_code, None);
    let offer = status.offer.expect("offered");
    assert_eq!(offer.version, "99.0.0");
    assert_eq!(offer.notes, "- Faster.");
    assert_eq!(fetcher.requests().len(), 1, "stable reads one file");
    let stored: rc_update::UpdateState = engine
        .db()
        .expect("db")
        .get(rc_update::STATE_KEY)
        .await
        .expect("read")
        .expect("stored");
    assert_eq!(stored.floors.stable, Some(42));
    assert_eq!(
        engine.update_notice(),
        Some(UpdateNotice::Available {
            version: "99.0.0".to_owned(),
            on_quit: false
        })
    );
}

/// No downgrade and not the running version: an older manifest offers nothing.
#[tokio::test]
async fn the_running_or_an_older_version_is_never_offered() {
    let (_root, engine, _fetcher) = served(Some(signed("0.0.1", 1))).await;
    let status = status_of(engine.dispatch(Command::CheckForUpdates {}).await);
    assert_eq!(status.offer, None);
    assert_eq!(engine.update_notice(), None);
}

/// A check that fails is quiet: no error of the command, its code in the status.
#[tokio::test]
async fn a_missing_manifest_is_recorded_as_its_code() {
    let (_root, engine, _fetcher) = served(None).await;
    let status = status_of(engine.dispatch(Command::CheckForUpdates {}).await);
    assert_eq!(status.last_code.as_deref(), Some("update.not_published"));
    assert!(status.last_check.is_some());
}

#[tokio::test]
async fn with_checks_off_the_task_asks_nothing_and_check_now_still_does() {
    let (_root, engine, fetcher) = served(Some(signed("99.0.0", 1))).await;
    engine
        .set_protected_setting(settings::UPDATE_CHECK_KEY, &json!(false))
        .await
        .expect("switch off");
    engine.scheduled_update_check().await;
    assert!(fetcher.requests().is_empty(), "{:?}", fetcher.requests());
    engine
        .dispatch(Command::CheckForUpdates {})
        .await
        .expect("check now");
    assert_eq!(fetcher.requests().len(), 1);
}

/// Every `update.*` key is protected, for the window's `settings.set` and for MCP alike.
#[tokio::test]
async fn settings_set_refuses_every_update_key() {
    let (_root, engine, _fetcher) = served(None).await;
    let keys = settings::SETTINGS
        .iter()
        .filter(|spec| spec.key.starts_with("update."))
        .map(|spec| spec.key);
    for key in keys.chain([rc_update::STATE_KEY]) {
        assert!(settings::is_protected(key), "{key}");
        let set = Command::SetSetting {
            key: key.to_owned(),
            value: json!(true),
        };
        let refused = engine.dispatch_mcp(set.clone()).await.expect_err("mcp");
        assert_eq!(refused.code(), "setting_protected", "{key}");
        let refused = engine.dispatch(set).await.expect_err("window");
        assert_eq!(refused.code(), "setting_protected", "{key}");
    }
}

/// An invalid stored value reads as the safe default; a stored channel wins over the version's.
#[tokio::test]
async fn invalid_stored_values_read_as_the_defaults() {
    let (_root, engine, _fetcher) = served(None).await;
    let db = engine.db().expect("db").clone();
    db.set(settings::UPDATE_INTERVAL_KEY, &json!(0))
        .await
        .expect("store");
    db.set(settings::UPDATE_CHANNEL_KEY, &json!("nightly"))
        .await
        .expect("store");
    db.set(settings::UPDATE_AUTO_KEY, &json!("always"))
        .await
        .expect("store");
    let config = engine.update_config().await;
    assert_eq!(config.interval_hours, 24);
    assert_eq!(config.channel, Channel::of_version(RUNNING));
    assert!(!config.channel_chosen);
    assert_eq!(config.auto, rc_update::auto_install::AutoMode::Off);
    db.set(settings::UPDATE_CHANNEL_KEY, &json!("beta"))
        .await
        .expect("store");
    let config = engine.update_config().await;
    assert_eq!(
        (config.channel, config.channel_chosen),
        (Channel::Beta, true)
    );
}

#[tokio::test]
async fn download_and_install_are_the_windows_only() {
    let (_root, engine, _fetcher) = served(None).await;
    for command in [Command::DownloadUpdate {}, Command::InstallUpdate {}] {
        let refused = engine.dispatch_mcp(command).await.expect_err("gui only");
        assert_eq!(refused.code(), "gui_only");
    }
    // Over MCP the check and the status are fine.
    engine
        .dispatch_mcp(Command::GetUpdateStatus {})
        .await
        .expect("status");
}

/// With a job in the queue there is no install, only the wait for the empty queue.
#[tokio::test]
async fn an_install_waits_for_the_empty_queue() {
    let (_root, engine, _fetcher) = served(None).await;
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
    let refused = engine
        .dispatch(Command::InstallUpdate {})
        .await
        .expect_err("busy");
    assert_eq!(refused.code(), "update.queue_busy");
}

/// A host that never answers holds nothing up: the status answers while the check waits.
#[tokio::test]
async fn a_hanging_host_blocks_nothing() {
    let (_root, engine) = served_with(Arc::new(HangingFetcher)).await;
    let checking = {
        let engine = engine.clone();
        tokio::spawn(async move { engine.dispatch(Command::CheckForUpdates {}).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    let status = tokio::time::timeout(Duration::from_secs(5), engine.update_status())
        .await
        .expect("the status does not wait for the check");
    assert_eq!(status.running_version, RUNNING);
    assert!(!checking.is_finished());
    checking.abort();
}

struct HangingFetcher;

#[async_trait::async_trait]
impl rc_update::Fetcher for HangingFetcher {
    async fn fetch(
        &self,
        _url: &rc_update::fetch::Url,
        _limit: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        std::future::pending().await
    }

    async fn open(
        &self,
        _url: &rc_update::fetch::Url,
    ) -> anyhow::Result<rc_update::fetch::Download> {
        std::future::pending().await
    }
}

/// The check asks for public files only: nothing of the installation, its id least of all.
#[tokio::test]
async fn a_check_sends_no_installation_id() {
    let (_root, engine, fetcher) = served(Some(signed("99.0.0", 1))).await;
    let id = engine
        .db()
        .expect("db")
        .installation_id()
        .await
        .expect("id");
    engine
        .dispatch(Command::CheckForUpdates {})
        .await
        .expect("check");
    for request in fetcher.requests() {
        assert!(!request.contains(&id), "{request}");
        assert!(!request.contains('?'), "{request}");
    }
}

#[test]
fn the_notice_shows_an_offer_until_later_and_updated_to_only_for_the_running_version() {
    let offer = rc_update::Offer {
        version: "99.0.0".to_owned(),
        channel: Channel::Stable,
        released_at: Utc::now(),
        notes: String::new(),
        artifact: None,
        schema_change: true,
    };
    assert_eq!(notice_of(RUNNING, None, None, None, None, false), None);
    assert!(matches!(
        notice_of(RUNNING, Some(&offer), None, None, None, true),
        Some(UpdateNotice::Available { on_quit: true, .. })
    ));
    assert_eq!(
        notice_of(RUNNING, Some(&offer), Some("99.0.0"), None, None, false),
        None,
        "Later puts it away"
    );
    assert_eq!(
        notice_of(RUNNING, None, None, Some(("99.0.0", "")), None, false),
        None,
        "not while another version runs"
    );
    assert!(matches!(
        notice_of(RUNNING, None, None, Some((RUNNING, "- New.")), None, false),
        Some(UpdateNotice::UpdatedTo { .. })
    ));
}

#[test]
fn every_refusal_code_is_named_once() {
    let codes = UpdateRefusal::all_codes();
    let mut unique = codes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), codes.len());
    assert!(codes.iter().all(|code| code.starts_with("update.")));
}

/// A package manager's installation (RC-100-52): the stable channel and no automatic install,
/// whatever is stored; portable and unknown keep the settings.
#[test]
fn a_managers_installation_reads_stable_and_never_installs_by_itself() {
    let stored = config::UpdateConfig {
        channel: Channel::Beta,
        auto: AutoMode::WhenIdle,
        ..config::UpdateConfig::defaults(RUNNING)
    };
    for kind in [
        rc_update::InstallKind::Scoop,
        rc_update::InstallKind::Homebrew,
        rc_update::InstallKind::Winget,
        rc_update::InstallKind::Deb,
        rc_update::InstallKind::Rpm,
    ] {
        let mut config = stored.clone();
        config.for_kind(kind);
        assert_eq!(config.channel, Channel::Stable, "{kind:?}");
        assert_eq!(config.auto, AutoMode::Off, "{kind:?}");
    }
    for kind in [
        rc_update::InstallKind::Portable,
        rc_update::InstallKind::Unknown,
    ] {
        let mut config = stored.clone();
        config.for_kind(kind);
        assert_eq!(config, stored, "{kind:?}");
    }
}
