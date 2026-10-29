use std::{path::Path, sync::Arc};

use rc_vfs::BackendId;

use super::{JobGuard, ReadGuard, ReadGuards, marked};
use crate::{JobKind, JobOptions, JobSpec};

struct Inside(bool);

impl ReadGuards for Inside {
    fn applies(&self) -> bool {
        self.0
    }

    fn guard(&self, _backend: &BackendId) -> ReadGuard {
        Arc::new(|_: &Path| None)
    }
}

fn spec(guarded: bool) -> JobSpec {
    JobSpec {
        kind: JobKind::Mkdir {
            path: rc_vfs::VfsPath::local("/x"),
        },
        options: JobOptions {
            read_guarded: guarded,
            ..JobOptions::default()
        },
    }
}

/// The queue's mark is exactly whether the guards apply now; without guards nothing is marked.
#[test]
fn a_job_is_marked_exactly_when_the_guards_apply() {
    let inside: Arc<dyn ReadGuards> = Arc::new(Inside(true));
    let outside: Arc<dyn ReadGuards> = Arc::new(Inside(false));
    for given in [false, true] {
        assert!(marked(spec(given), Some(&inside)).options.read_guarded);
        assert!(!marked(spec(given), Some(&outside)).options.read_guarded);
        assert!(!marked(spec(given), None).options.read_guarded);
    }
}

/// A guarded job of a queue that was given no guard reads nothing (fails closed); an unguarded
/// job has no guard at all.
#[test]
fn a_guarded_job_without_guards_reads_nothing() {
    assert!(JobGuard::of(false, None).is_none());
    let closed = JobGuard::of(true, None).expect("guarded");
    assert!(closed.on(&BackendId::Local)(Path::new("/any")).is_some());
    let inside: Arc<dyn ReadGuards> = Arc::new(Inside(true));
    let open = JobGuard::of(true, Some(&inside)).expect("guarded");
    assert!(open.on(&BackendId::Local)(Path::new("/any")).is_none());
}

/// The mark is kept across a restart but left out of the JSON when unset.
#[test]
fn the_mark_is_stored_only_when_set() {
    let plain = serde_json::to_value(JobOptions::default()).expect("json");
    assert!(plain.get("read_guarded").is_none(), "{plain}");
    let guarded = JobOptions {
        read_guarded: true,
        ..JobOptions::default()
    };
    let text = serde_json::to_string(&guarded).expect("json");
    let back: JobOptions = serde_json::from_str(&text).expect("reads");
    assert!(back.read_guarded);
}
