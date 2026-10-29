//! `update.download` and `update.install` (RC-100-05, RC-100-06), both `GuiOnly`: the verified
//! download of the offer, and the hand-over to the updater.
//!
//! The hand-over: the program folder writable with twice the archive free, the database copy
//! written and checked (`rc_db` backup into `<data>/update/pre-update/`, its BLAKE3 into the
//! journal - always, the way back must never be missing), the journal in phase `handed`, the
//! updater started from a copy of this program. A running, waiting or paused job never is
//! interrupted: the install waits for the empty queue and the window asks again.

use chrono::Utc;
use rc_update::install::{
    self, DEFAULT_CONFIRM_TIMEOUT_SECS, DOWNLOAD_DIR, Journal, PRE_UPDATE_DIR, Phase, Plan,
    process, steps, update_dir,
};

use super::{DownloadState, RUNNING, UpdateRefusal};
use crate::{Engine, EngineError, EngineEvent, Outcome};

/// How often a running download reports its bytes.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

impl Engine {
    /// `update.download`: fetches the offer's archive in the background; the status says how
    /// far it is.
    pub(crate) async fn download_update(&self) -> Result<Outcome, EngineError> {
        self.start_update_download().map_err(Self::update_refused)?;
        self.update_status_outcome().await
    }

    /// `update.install`: hands the downloaded offer to the updater and asks the window to end.
    pub(crate) async fn install_update(&self) -> Result<Outcome, EngineError> {
        if self.jobs_unfinished() {
            return Err(Self::update_refused(UpdateRefusal::new(
                "update.queue_busy",
                "jobs run or wait in the queue; the update installs once it is empty",
            )));
        }
        self.hand_over_update(true)
            .await
            .map_err(Self::update_refused)?;
        // The window ends as `app.quit` ends it; the queue is empty.
        self.publish(EngineEvent::Quit { unfinished: 0 });
        self.update_status_outcome().await
    }

    /// Whether a job runs, waits or is paused in the queue.
    pub(crate) fn jobs_unfinished(&self) -> bool {
        self.inner
            .ops
            .jobs()
            .iter()
            .any(|job| !job.state.is_finished())
    }

    /// Starts the download of the offer, or finds the verified file already there.
    pub(crate) fn start_update_download(&self) -> Result<(), UpdateRefusal> {
        let data_dir = self.started()?.data_dir.clone();
        let (artifact, progress) = {
            let mut state = self.desk_state();
            let offer = state.stored.offer.clone().ok_or_else(|| {
                UpdateRefusal::new("update.no_offer", "no newer version is offered")
            })?;
            let artifact = offer.artifact.ok_or_else(|| {
                UpdateRefusal::new(
                    "update.no_artifact",
                    "the release has no package for this platform",
                )
            })?;
            match state.download {
                DownloadState::Running { .. } => {
                    return Err(UpdateRefusal::new(
                        "update.download_running",
                        "the download runs already",
                    ));
                }
                DownloadState::Ready { .. } => return Ok(()),
                DownloadState::None | DownloadState::Failed { .. } => {}
            }
            let progress = rc_update::Progress::default();
            state.download = DownloadState::Running {
                received: 0,
                total: artifact.size,
            };
            state.progress = Some(progress.clone());
            (artifact, progress)
        };
        self.publish(EngineEvent::Update);
        let engine = self.clone();
        tokio::spawn(async move {
            let folder = update_dir(&data_dir).join(DOWNLOAD_DIR);
            let result = match rc_update::verified_file(&artifact, &folder).await {
                Some(path) => Ok(path),
                None => {
                    let free = rc_vfs::disk_space(&data_dir).ok().map(|(free, _)| free);
                    match rc_update::check_space(&artifact, free) {
                        Ok(()) => {
                            let config = engine.update_config().await;
                            let fetcher = engine.update_fetcher(&config);
                            let ticker = engine.report_download(progress.clone(), artifact.size);
                            let downloaded = rc_update::download_verified(
                                fetcher.as_ref(),
                                &artifact,
                                &folder,
                                &progress,
                            )
                            .await;
                            ticker.abort();
                            downloaded
                        }
                        Err(error) => Err(error),
                    }
                }
            };
            {
                let mut state = engine.desk_state();
                state.progress = None;
                state.download = match result {
                    Ok(path) => DownloadState::Ready {
                        file: path.display().to_string(),
                    },
                    Err(error) => {
                        tracing::warn!(code = error.code(), %error, "the update download failed");
                        DownloadState::Failed {
                            code: error.code().to_owned(),
                        }
                    }
                };
            }
            engine.publish(EngineEvent::Update);
        });
        Ok(())
    }

    /// Announces the download's bytes every [`PROGRESS_EVERY`] until it is aborted.
    fn report_download(
        &self,
        progress: rc_update::Progress,
        total: u64,
    ) -> tokio::task::JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PROGRESS_EVERY).await;
                let received = progress.received();
                if let DownloadState::Running {
                    received: shown, ..
                } = &mut engine.desk_state().download
                {
                    *shown = received;
                }
                engine.publish(EngineEvent::UpdateProgress { received, total });
            }
        })
    }

    /// The hand-over: preflight, database copy, journal, updater. `start_new` is false for
    /// "install when quitting": the next start a person makes proves the new version.
    pub(crate) async fn hand_over_update(&self, start_new: bool) -> Result<(), UpdateRefusal> {
        let started = self.started()?;
        if started.installation.kind != rc_update::InstallKind::Portable {
            return Err(UpdateRefusal::new(
                started
                    .installation
                    .reason
                    .unwrap_or("update.install_unsupported"),
                "this installation does not update itself",
            ));
        }
        let (offer, handed) = {
            let state = self.desk_state();
            (state.stored.offer.clone(), state.handed)
        };
        if handed {
            return Err(UpdateRefusal::new(
                "update.handed",
                "the update is handed to the updater already",
            ));
        }
        let offer = offer
            .ok_or_else(|| UpdateRefusal::new("update.no_offer", "no newer version is offered"))?;
        let artifact = offer.artifact.clone().ok_or_else(|| {
            UpdateRefusal::new("update.no_artifact", "no package for this platform")
        })?;
        let data_dir = started.data_dir.clone();
        let download = update_dir(&data_dir).join(DOWNLOAD_DIR);
        let file = rc_update::verified_file(&artifact, &download)
            .await
            .ok_or_else(|| {
                UpdateRefusal::new(
                    "update.not_downloaded",
                    "the verified download is not there",
                )
            })?;
        let install_dir = started.installation.install_dir.clone();
        let free = rc_vfs::disk_space(&install_dir).ok().map(|(free, _)| free);
        install::preflight(&install_dir, artifact.size, free)?;
        let db = self.db().map_err(|_| {
            UpdateRefusal::new("update.not_started", "the database is not attached")
        })?;
        let database = db
            .path()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| UpdateRefusal::new("update.not_started", "the database is in memory"))?;
        // The database copy first, always: the way back must never be missing.
        let pre_update = update_dir(&data_dir).join(PRE_UPDATE_DIR);
        std::fs::create_dir_all(&pre_update)
            .map_err(|error| UpdateRefusal::new("update.failed", error.to_string()))?;
        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
        let target = pre_update.join(format!("rcommander-{RUNNING}-{stamp}.db"));
        let copy = db
            .backup_to(&target, false)
            .await
            .map_err(|error| UpdateRefusal::new(error.code(), error.to_string()))?
            .path;
        let copy_for_digest = copy.clone();
        let blake3 = tokio::task::spawn_blocking(move || steps::blake3_of(&copy_for_digest))
            .await
            .map_err(|error| UpdateRefusal::new("update.failed", error.to_string()))?
            .map_err(|error| UpdateRefusal::new("update.failed", format!("{error:#}")))?;
        let plan = Plan {
            kind: started.installation.kind,
            from_version: RUNNING.to_owned(),
            target_version: offer.version.clone(),
            artifact: file,
            sha256: artifact.sha256.clone(),
            size: artifact.size,
            install_dir,
            executable: started.installation.executable.clone(),
            data_dir: data_dir.clone(),
            database,
            database_copy: Some(copy),
            database_copy_blake3: Some(blake3),
            args: std::env::args().skip(1).collect(),
            start_new,
            confirm_timeout_secs: DEFAULT_CONFIRM_TIMEOUT_SECS,
        };
        plan.validate()?;
        let mut journal = Journal::begin(plan);
        journal
            .write()
            .map_err(|error| UpdateRefusal::new("update.failed", format!("{error:#}")))?;
        if let Err(error) = process::launch_updater(&journal) {
            let detail = format!("{error:#}");
            let _ = journal.end(Phase::Failed, "update.updater_did_not_start", &detail);
            return Err(UpdateRefusal::new("update.updater_did_not_start", detail));
        }
        self.desk_state().handed = true;
        tracing::info!(version = %offer.version, start_new, "the update is handed to the updater");
        self.publish(EngineEvent::Update);
        Ok(())
    }
}
