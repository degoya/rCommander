use std::path::PathBuf;

use rc_ops::{JobKind, JobOptions, JobSnapshot, JobState, Progress};
use rc_vfs::{BackendId, VfsPath};

use super::{device_files, device_target, in_transfer, writing_to};

fn phone(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Device {
            device: "phone".to_owned(),
        },
        path: PathBuf::from(path),
    }
}

fn copy(sources: &[&str], target: VfsPath) -> JobKind {
    JobKind::Copy {
        sources: sources.iter().map(|path| VfsPath::local(*path)).collect(),
        target,
    }
}

fn job(kind: JobKind, state: JobState) -> JobSnapshot {
    JobSnapshot {
        id: 1,
        kind,
        options: JobOptions::default(),
        state,
        progress: Progress::default(),
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: None,
        resumable: false,
        batch: None,
    }
}

#[test]
fn only_a_copy_or_move_onto_a_device_has_a_device_target() {
    let onto = copy(&["/pc/a.jpg"], phone("/Internal storage/DCIM"));
    assert_eq!(device_target(&onto), Some(&phone("/Internal storage/DCIM")));
    let off = JobKind::Copy {
        sources: vec![phone("/Internal storage/DCIM/a.jpg")],
        target: VfsPath::local("/pc"),
    };
    assert_eq!(device_target(&off), None, "a copy off the device");
    let mkdir = JobKind::Mkdir {
        path: phone("/Internal storage/new"),
    };
    assert_eq!(device_target(&mkdir), None, "no transfer");
}

#[test]
fn the_files_of_a_transfer_are_its_sources_names_in_the_target_or_the_new_name() {
    let into = copy(&["/pc/a.jpg", "/pc/b.jpg"], phone("/Internal storage/DCIM"));
    assert_eq!(
        device_files(&into),
        [
            phone("/Internal storage/DCIM/a.jpg"),
            phone("/Internal storage/DCIM/b.jpg")
        ]
    );
    let renamed = JobKind::MoveAs {
        source: VfsPath::local("/pc/a.jpg"),
        target: phone("/Internal storage/DCIM/c.jpg"),
    };
    assert_eq!(
        device_files(&renamed),
        [phone("/Internal storage/DCIM/c.jpg")]
    );
    assert!(device_files(&copy(&["/pc/a.jpg"], VfsPath::local("/other"))).is_empty());
}

/// F5 onto a file a job is still putting onto the phone: found as in transfer, whatever the
/// first job's state before its end; a finished job, another name or another folder is not.
#[test]
fn a_file_an_unfinished_job_writes_onto_the_device_is_in_transfer() {
    let dcim = phone("/Internal storage/DCIM");
    let again = copy(&["/pc/a.jpg"], dcim.clone());
    for state in [
        JobState::Queued,
        JobState::Counting,
        JobState::Running,
        JobState::Paused,
        JobState::Waiting,
    ] {
        let running = [job(copy(&["/pc/a.jpg", "/pc/b.jpg"], dcim.clone()), state)];
        assert_eq!(
            in_transfer(&again, &running),
            Some(phone("/Internal storage/DCIM/a.jpg")),
            "{state:?}"
        );
    }
    for state in [JobState::Completed, JobState::Cancelled, JobState::Failed] {
        let over = [job(copy(&["/pc/a.jpg"], dcim.clone()), state)];
        assert_eq!(in_transfer(&again, &over), None, "{state:?}");
    }
    let running = [job(copy(&["/pc/a.jpg"], dcim.clone()), JobState::Running)];
    let other_name = copy(&["/pc/c.jpg"], dcim.clone());
    assert_eq!(in_transfer(&other_name, &running), None);
    let other_folder = copy(&["/pc/a.jpg"], phone("/Internal storage/Download"));
    assert_eq!(in_transfer(&other_folder, &running), None);
    // The same name from another folder of the machine lands on the same file.
    let other_source = copy(&["/elsewhere/a.jpg"], dcim.clone());
    assert_eq!(
        in_transfer(&other_source, &running),
        Some(phone("/Internal storage/DCIM/a.jpg"))
    );
    // A copy under a new name onto the file a job writes.
    let as_name = JobKind::CopyAs {
        source: VfsPath::local("/pc/x.jpg"),
        target: phone("/Internal storage/DCIM/a.jpg"),
    };
    assert_eq!(
        in_transfer(&as_name, &running),
        Some(phone("/Internal storage/DCIM/a.jpg"))
    );
}

#[test]
fn a_copy_onto_a_disk_is_never_in_transfer() {
    let disk = VfsPath::local("/dst");
    let running = [job(copy(&["/pc/a.jpg"], disk.clone()), JobState::Running)];
    assert_eq!(in_transfer(&copy(&["/pc/a.jpg"], disk), &running), None);
}

#[test]
fn a_device_is_written_to_while_a_job_onto_it_has_not_ended() {
    let dcim = phone("/Internal storage/DCIM");
    let jobs = [
        job(copy(&["/pc/a.jpg"], dcim.clone()), JobState::Completed),
        job(copy(&["/pc/b.jpg"], dcim.clone()), JobState::Running),
    ];
    let busy = writing_to(&dcim.backend, &jobs).expect("the running job");
    assert_eq!(busy.state, JobState::Running);
    let other = BackendId::Device {
        device: "tablet".to_owned(),
    };
    assert!(writing_to(&other, &jobs).is_none(), "another device");
    assert!(writing_to(&BackendId::Local, &jobs).is_none(), "no device");
    assert!(writing_to(&dcim.backend, &jobs[..1]).is_none(), "it ended");
}
