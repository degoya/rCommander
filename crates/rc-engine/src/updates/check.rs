//! `update.check` and `update.status` (RC-100-05): one check against the release host and what
//! the engine answers about the self-update.
//!
//! A check fetches the manifests of the channel in force, verifies them against the compiled-in
//! root and the stored floors, stores the floors **before** it acts on the result, and keeps the
//! verified offer. It sends nothing but a `GET` per file, with `rCommander/<version>` as the user
//! agent - no cookie, no query, no installation id. A refusal is quiet: its code and time.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use rc_update::{
    Fetcher, HttpFetcher, ProxyChoice, Sources, Target,
    install::{Journal, recover},
    manifest,
};

use super::{
    DownloadState, LastUpdate, RUNNING, UpdateRefusal, UpdateStatus, config::UpdateConfig,
};
use crate::{Engine, EngineError, EngineEvent, Outcome};

impl Engine {
    /// `update.check`: checks now, whether checks by itself are on or not, and answers the
    /// status. A refused check is no error of the command: its code is in the status.
    pub(crate) async fn check_for_updates(&self) -> Result<Outcome, EngineError> {
        self.run_update_check().await?;
        self.update_status_outcome().await
    }

    /// `update.status`.
    pub(crate) async fn update_status_outcome(&self) -> Result<Outcome, EngineError> {
        Ok(Outcome::Update(Box::new(self.update_status().await)))
    }

    /// The check the background task makes: only with `update.check_enabled` on.
    pub(crate) async fn scheduled_update_check(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        if !UpdateConfig::read(db, RUNNING).await.check_enabled {
            return;
        }
        if let Err(error) = self.run_update_check().await {
            tracing::warn!(code = error.code(), "the update check did not run");
        }
    }

    /// The configuration in force: the stored one, the defaults without a database.
    pub(crate) async fn update_config(&self) -> UpdateConfig {
        let mut config = match self.db() {
            Ok(db) => UpdateConfig::read(db, RUNNING).await,
            Err(_) => UpdateConfig::defaults(RUNNING),
        };
        // A package manager's installation (RC-100-52): its channel carries plain releases only,
        // and it never installs by itself, whatever the stored settings say.
        if let Some(started) = self.desk().started.get() {
            config.for_kind(started.installation.kind);
        }
        self.desk_state().auto = config.auto;
        config
    }

    /// The fetcher for `config`: the test's, or the release host through the proxy setting.
    pub(crate) fn update_fetcher(&self, config: &UpdateConfig) -> Arc<dyn Fetcher> {
        if let Some(fetcher) = self.desk().fetcher.get() {
            return Arc::clone(fetcher);
        }
        let proxy = ProxyChoice::parse(&config.proxy).unwrap_or(ProxyChoice::System);
        Arc::new(HttpFetcher::new(&proxy))
    }

    async fn run_update_check(&self) -> Result<(), EngineError> {
        self.db()?;
        let _one = self.desk().checking.lock().await;
        self.load_update_state().await;
        let config = self.update_config().await;
        let now = Utc::now();
        let trust = match self.desk().trust.get() {
            Some(trust) => Ok(trust.clone()),
            None => manifest::release_trust(now),
        };
        let sources = Sources::for_setting(&config.source);
        let (trust, sources) = match (trust, sources) {
            (Ok(trust), Ok(sources)) => (trust, sources),
            (Err(error), _) | (_, Err(error)) => {
                self.record_check(now, Some(error.code()), None).await;
                return Ok(());
            }
        };
        let fetcher = self.update_fetcher(&config);
        let floors = self.desk_state().stored.floors;
        let report = rc_update::check(
            fetcher.as_ref(),
            &sources,
            &trust,
            config.channel,
            floors,
            now,
        )
        .await;
        // The floors first: a replay must not be accepted because the result was lost.
        self.store_update_state(|stored| stored.floors = report.floors)
            .await;
        let offer = rc_update::newest_offer(
            RUNNING,
            config.channel,
            &report.manifests,
            &Target::current(),
        );
        let code = report.problem.as_ref().map(rc_update::UpdateError::code);
        self.record_check(now, code, offer).await;
        // With the automation on, the offer is fetched in the background (RC-100-07).
        if config.auto != rc_update::auto_install::AutoMode::Off
            && let Err(refusal) = self.start_update_download()
        {
            tracing::debug!(
                code = refusal.code(),
                "no background download of the update"
            );
        }
        Ok(())
    }

    /// Stores the end of a check: when, its code, the offer; the notice put away is shown again.
    async fn record_check(
        &self,
        now: chrono::DateTime<Utc>,
        code: Option<&'static str>,
        offer: Option<rc_update::Offer>,
    ) {
        {
            let mut state = self.desk_state();
            state.dismissed = None;
            // A download of another version is no longer the offer's.
            if offer.as_ref().map(|offer| &offer.version)
                != state.stored.offer.as_ref().map(|offer| &offer.version)
                && !matches!(state.download, DownloadState::Running { .. })
            {
                state.download = DownloadState::None;
            }
        }
        self.store_update_state(|stored| {
            stored.last_check = Some(now);
            stored.last_code = code.map(str::to_owned);
            stored.offer = offer;
        })
        .await;
        self.publish(EngineEvent::Update);
    }

    /// What is known about the self-update.
    pub async fn update_status(&self) -> UpdateStatus {
        self.load_update_state().await;
        let config = self.update_config().await;
        let started = self.desk().started.get();
        let journal = started
            .and_then(|started| Journal::read(&started.data_dir).ok().flatten())
            .map(|journal| LastUpdate {
                phase: journal.phase.as_str().to_owned(),
                from_version: journal.plan.from_version,
                target_version: journal.plan.target_version,
                reason: journal.reason,
            });
        let leftover_bytes = started.map_or(0, |started| {
            rc_update::retention::leftover_bytes(
                &started.data_dir,
                &started.installation.install_dir,
            )
        });
        let queue_busy = self.jobs_unfinished();
        let state = self.desk_state();
        UpdateStatus {
            running_version: RUNNING.to_owned(),
            channel: config.channel.as_str().to_owned(),
            channel_chosen: config.channel_chosen,
            check_enabled: config.check_enabled,
            interval_hours: config.interval_hours,
            source: if config.source.is_empty() {
                "official".to_owned()
            } else {
                config.source.clone()
            },
            last_check: state.stored.last_check,
            last_code: state.stored.last_code.clone(),
            offer: state.stored.offer.clone(),
            download: state.download.clone(),
            install_kind: started
                .map_or("unknown", |started| started.installation.kind.as_str())
                .to_owned(),
            install_reason: match started {
                Some(started) => started.installation.reason.map(str::to_owned),
                None => Some("update.not_started".to_owned()),
            },
            install_command: started
                .and_then(|started| started.installation.kind.update_command())
                .map(str::to_owned),
            installing: state.handed,
            queue_busy,
            last_update: journal,
            updated_to: state
                .updated_to
                .as_ref()
                .map(|(version, _)| version.clone()),
            auto_install: config.auto.as_str().to_owned(),
            auto_waiting_for: state.waiting.map(|wait| wait.as_str().to_owned()),
            leftover_bytes,
        }
    }

    /// Proves an update that waited for this start and notes what the window says about it once:
    /// "Updated to X" with the notes, or why the last one did not go ahead (a `warn` line).
    pub(crate) async fn note_update_result(&self, data_dir: &Path) {
        match recover::confirm_started(data_dir, RUNNING) {
            Ok(true) => tracing::info!(version = RUNNING, "this start proved the update"),
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "the update journal could not be read"),
        }
        let Ok(Some(journal)) = Journal::read(data_dir) else {
            return;
        };
        let config = self.update_config().await;
        let mark = format!("{}:{}", journal.phase.as_str(), journal.plan.target_version);
        let announced = self.desk_state().stored.announced.clone();
        if announced.as_deref() == Some(mark.as_str()) || !journal.phase.is_terminal() {
            return;
        }
        let target = journal.plan.target_version.clone();
        match journal.phase {
            rc_update::install::Phase::Verified if target == RUNNING => {
                let notes = self
                    .desk_state()
                    .stored
                    .offer
                    .as_ref()
                    .filter(|offer| offer.version == RUNNING)
                    .map(|offer| offer.notes.clone())
                    .unwrap_or_default();
                if config.whats_new {
                    self.desk_state().updated_to = Some((target, notes));
                }
            }
            rc_update::install::Phase::RolledBack | rc_update::install::Phase::Failed => {
                let reason = journal
                    .reason
                    .clone()
                    .unwrap_or_else(|| "update.failed".to_owned());
                tracing::warn!(version = %target, code = %reason, "the update did not go ahead");
                self.desk_state().failed = Some((target.clone(), reason));
                self.store_update_state(|stored| stored.record_failure(&target))
                    .await;
            }
            _ => {}
        }
        self.store_update_state(|stored| stored.announced = Some(mark))
            .await;
    }

    /// The refusal of an update command as the engine's error.
    pub(crate) fn update_refused(refusal: UpdateRefusal) -> EngineError {
        EngineError::Update(refusal)
    }
}
