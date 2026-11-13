use std::path::PathBuf;

use rc_engine::{
    BackendId, Command, JobChange, JobId, JobKind, JobOptions, JobSnapshot, JobState, PanelId,
    Progress, VfsPath,
};

use super::{super::FollowUp, transfer_on};
use crate::{
    app::{Effect, harness::Harness},
    i18n::error_text,
};

fn phone(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Device {
            device: "phone".to_owned(),
        },
        path: PathBuf::from(path),
    }
}

/// A running copy of `/pc/<name>` into `target`, `done` of 200 bytes copied.
fn copy(id: JobId, name: &str, target: VfsPath, done: u64) -> JobSnapshot {
    JobSnapshot {
        id,
        kind: JobKind::Copy {
            sources: vec![VfsPath::local(format!("/pc/{name}"))],
            target,
        },
        options: JobOptions::default(),
        state: JobState::Running,
        progress: Progress {
            bytes_total: 200,
            bytes_done: done,
            ..Progress::default()
        },
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: None,
        resumable: false,
        batch: None,
    }
}

/// The panels `effects` read again.
fn refreshed(effects: &[Effect]) -> Vec<PanelId> {
    let mut panels = Vec::new();
    for effect in effects {
        let Effect::Dispatch(_, commands) = effect else {
            continue;
        };
        for command in commands {
            if let Command::Refresh { panel: Some(panel) } = command {
                panels.push(*panel);
            }
        }
    }
    panels
}

fn finished() -> JobChange {
    JobChange::Finished {
        state: JobState::Completed,
    }
}

#[test]
fn the_hint_names_the_file_at_work_and_the_share_done() {
    let dcim = phone("/Internal storage/DCIM");
    let mut job = copy(1, "a.mp4", dcim.clone(), 0);
    assert_eq!(
        transfer_on(&dcim.backend, [&job]),
        Some(("a.mp4".to_owned(), 0)),
        "the first source before the first file"
    );
    job.progress.current = Some(VfsPath::local("/pc/sub/b.mp4"));
    job.progress.bytes_done = 199;
    assert_eq!(
        transfer_on(&dcim.backend, [&job]),
        Some(("b.mp4".to_owned(), 99)),
        "rounded down"
    );
    job.progress.bytes_done = 200;
    assert_eq!(
        transfer_on(&dcim.backend, [&job]),
        Some(("b.mp4".to_owned(), 100))
    );
    assert_eq!(transfer_on(&BackendId::Local, [&job]), None);
    job.state = JobState::Completed;
    assert_eq!(transfer_on(&dcim.backend, [&job]), None, "over");
}

/// While a job writes onto the phone, the panel showing it says so and the other one does not;
/// when the job is over the hint and a refused Ctrl+R's message go, and both panels are read.
#[tokio::test]
async fn a_panel_on_the_device_says_that_a_transfer_runs_until_it_is_over() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let dcim = phone("/Internal storage/DCIM");
    harness.app.pane_mut(PanelId::Right).state.path = dcim.clone();
    let jobs = &mut harness.app.file_ops.jobs;
    let _ = jobs.adopt(
        copy(911, "a.mp4", dcim, 50),
        PanelId::Left,
        FollowUp::Refresh,
        false,
    );
    harness.app.show_device_hints();
    let hint = harness.app.pane(PanelId::Right).transfer_hint.clone();
    let hint = hint.expect("the panel on the phone has the hint");
    assert!(hint.contains("a.mp4") && hint.contains("25"), "{hint}");
    assert_eq!(harness.app.pane(PanelId::Left).transfer_hint, None);

    harness.app.pane_mut(PanelId::Right).error = Some(error_text("device_busy"));
    let effects = harness.app.on_job(911, finished());
    assert_eq!(harness.app.pane(PanelId::Right).transfer_hint, None);
    assert_eq!(harness.app.pane(PanelId::Right).error, None);
    assert_eq!(refreshed(&effects), [PanelId::Left, PanelId::Right]);
}

/// A job that ends while another still writes onto the phone reads only the other panel again:
/// the phone's reading would be refused, and it is read when its own job is over.
#[tokio::test]
async fn a_device_still_written_to_is_not_read_when_another_job_ends() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let dcim = phone("/Internal storage/DCIM");
    harness.app.pane_mut(PanelId::Right).state.path = dcim.clone();
    let jobs = &mut harness.app.file_ops.jobs;
    let _ = jobs.adopt(
        copy(921, "a.mp4", dcim, 50),
        PanelId::Left,
        FollowUp::Refresh,
        false,
    );
    let on_disk = copy(922, "b.txt", VfsPath::local("/dst"), 50);
    let _ = jobs.adopt(on_disk, PanelId::Left, FollowUp::Refresh, false);
    let effects = harness.app.on_job(922, finished());
    assert_eq!(refreshed(&effects), [PanelId::Left]);
    assert!(harness.app.pane(PanelId::Right).transfer_hint.is_some());
}

/// `panels.device_transfer_hint` off: no hint, and the panels are read as before.
#[tokio::test]
async fn with_the_setting_off_no_panel_has_the_hint() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let dcim = phone("/Internal storage/DCIM");
    harness.app.pane_mut(PanelId::Right).state.path = dcim.clone();
    harness.app.file_ops.hide_device_hint = true;
    let jobs = &mut harness.app.file_ops.jobs;
    let _ = jobs.adopt(
        copy(931, "a.mp4", dcim, 50),
        PanelId::Left,
        FollowUp::Refresh,
        false,
    );
    let on_disk = copy(932, "b.txt", VfsPath::local("/dst"), 50);
    let _ = jobs.adopt(on_disk, PanelId::Left, FollowUp::Refresh, false);
    harness.app.show_device_hints();
    assert_eq!(harness.app.pane(PanelId::Right).transfer_hint, None);
    let effects = harness.app.on_job(932, finished());
    assert_eq!(refreshed(&effects), [PanelId::Left, PanelId::Right]);
}
