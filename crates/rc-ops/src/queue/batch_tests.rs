use rc_vfs::VfsPath;

use super::per_item;
use crate::{ConflictChoice, DeleteMode, JobKind, JobOptions, JobSpec, PackFormat, PackSettings};

fn options() -> JobOptions {
    JobOptions {
        on_conflict: Some(ConflictChoice::Skip),
        verify: true,
        ..JobOptions::default()
    }
}

fn paths(names: &[&str]) -> Vec<VfsPath> {
    names.iter().map(|name| VfsPath::local(*name)).collect()
}

#[test]
fn several_sources_become_one_job_each_in_their_order() {
    let target = VfsPath::local("/out");
    let copy = JobSpec {
        kind: JobKind::Copy {
            sources: paths(&["/a/x", "/a/y", "/a/z.txt"]),
            target: target.clone(),
        },
        options: options(),
    };
    let jobs = per_item(copy);
    let expected: Vec<JobSpec> = ["/a/x", "/a/y", "/a/z.txt"]
        .iter()
        .map(|source| JobSpec {
            kind: JobKind::Copy {
                sources: paths(&[*source]),
                target: target.clone(),
            },
            options: options(),
        })
        .collect();
    assert_eq!(jobs, expected);

    let moving = JobSpec {
        kind: JobKind::Move {
            sources: paths(&["/a/x", "/a/y"]),
            target: target.clone(),
        },
        options: options(),
    };
    let jobs = per_item(moving);
    assert_eq!(jobs.len(), 2);
    assert!(matches!(&jobs[1].kind, JobKind::Move { sources, .. } if sources == &paths(&["/a/y"])));

    let delete = JobSpec {
        kind: JobKind::Delete {
            sources: paths(&["/a/x", "/a/y"]),
            mode: DeleteMode::Permanent,
        },
        options: JobOptions::default(),
    };
    let jobs = per_item(delete);
    assert_eq!(jobs.len(), 2);
    assert!(matches!(
        &jobs[0].kind,
        JobKind::Delete { sources, mode: DeleteMode::Permanent } if sources == &paths(&["/a/x"])
    ));
}

#[test]
fn one_source_and_an_archive_of_several_stay_one_job() {
    let single = JobSpec {
        kind: JobKind::Copy {
            sources: paths(&["/a/x"]),
            target: VfsPath::local("/out"),
        },
        options: JobOptions::default(),
    };
    assert_eq!(per_item(single.clone()), vec![single]);
    let pack = JobSpec {
        kind: JobKind::Pack {
            sources: paths(&["/a/x", "/a/y"]),
            target: VfsPath::local("/out/a.zip"),
            settings: PackSettings {
                format: PackFormat::Zip,
                ..PackSettings::default()
            },
        },
        options: JobOptions::default(),
    };
    assert_eq!(per_item(pack.clone()), vec![pack]);
}
