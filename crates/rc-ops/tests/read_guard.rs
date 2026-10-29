//! The read guard at run time (RC-080-42): a job submitted while the guard applies (an MCP call)
//! reads no file it refuses - that file fails as `sensitive_path`, the others copy -, judged
//! right before each file is opened, so a file that became one after the submission is refused
//! too. A job submitted while it does not apply (the window's) is not guarded.

mod support;

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use rc_ops::{ErrorPolicy, JobKind, JobOptions, JobSpec, JobState, ReadGuard, ReadGuards};
use rc_vfs::BackendId;
use support::{Harness, exists, mem, write};

/// Refuses the file names in `names`; applies while `mcp` is set.
#[derive(Default)]
struct Names {
    names: Arc<Mutex<Vec<String>>>,
    mcp: AtomicBool,
}

impl Names {
    fn refuse(&self, name: &str) {
        self.names.lock().expect("names").push(name.to_owned());
    }
}

impl ReadGuards for Names {
    fn applies(&self) -> bool {
        self.mcp.load(Ordering::SeqCst)
    }

    fn guard(&self, _backend: &BackendId) -> ReadGuard {
        let names = Arc::clone(&self.names);
        Arc::new(move |path: &Path| {
            let name = path.file_name()?.to_str()?;
            names
                .lock()
                .expect("names")
                .iter()
                .any(|refused| refused == name)
                .then_some("a key")
        })
    }
}

fn copy(sources: &[&str], target: &str) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: sources.iter().map(|path| mem(1, path)).collect(),
            target: mem(2, target),
        },
        options: JobOptions {
            on_error: ErrorPolicy::Skip,
            ..JobOptions::default()
        },
    }
}

/// `Memory(1)` with three files, `Memory(2)` empty, and the guards installed.
async fn setup(mcp: bool) -> (Harness, Arc<Names>, rc_vfs::MemoryFs) {
    let harness = Harness::start();
    let one = harness.memory(1);
    let two = harness.memory(2);
    for name in ["a.txt", "id_rsa", "b.txt"] {
        write(&one, &format!("/src/{name}"), name.as_bytes()).await;
    }
    let names = Arc::new(Names::default());
    names.mcp.store(mcp, Ordering::SeqCst);
    harness
        .queue
        .set_read_guards(Arc::clone(&names) as Arc<dyn ReadGuards>);
    (harness, names, two)
}

#[tokio::test]
async fn a_refused_file_fails_and_the_others_copy() {
    let (harness, names, two) = setup(true).await;
    names.refuse("id_rsa");
    let done = harness.run(copy(&["/src"], "/dst")).await;
    assert!(done.options.read_guarded, "submitted inside an MCP call");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    // A path, not a string: the walk joins with the platform's separator (`/src\id_rsa` on
    // Windows).
    let codes: Vec<(&str, &Path)> = done
        .failures
        .iter()
        .map(|failure| (failure.code.as_str(), failure.path.path.as_path()))
        .collect();
    assert_eq!(codes, [("sensitive_path", Path::new("/src/id_rsa"))]);
    assert!(exists(&two, "/dst/src/a.txt").await);
    assert!(exists(&two, "/dst/src/b.txt").await);
    assert!(
        !exists(&two, "/dst/src/id_rsa").await,
        "nothing of it is written"
    );
}

#[tokio::test]
async fn a_file_that_became_sensitive_after_the_submission_is_refused() {
    let (harness, names, two) = setup(true).await;
    let list = harness.queue.create_list("Later").expect("created").id;
    let id = harness
        .queue
        .submit_into(copy(&["/src"], "/dst"), None, list)
        .expect("accepted")
        .id;
    // Judged fine when submitted; a key by the time the job opens it.
    names.refuse("b.txt");
    harness.queue.start_list(list).expect("started");
    let done = harness.finish(id).await;
    assert_eq!(done.failures.len(), 1, "{done:?}");
    assert_eq!(done.failures[0].code, "sensitive_path");
    assert!(
        exists(&two, "/dst/src/id_rsa").await,
        "not refused by this guard"
    );
    assert!(!exists(&two, "/dst/src/b.txt").await);
}

#[tokio::test]
async fn a_job_of_the_window_is_not_guarded() {
    let (harness, names, two) = setup(false).await;
    names.refuse("id_rsa");
    // A caller's own mark counts for nothing: only the queue marks a job.
    let mut spec = copy(&["/src"], "/dst");
    spec.options.read_guarded = true;
    let done = harness.run(spec).await;
    assert!(!done.options.read_guarded);
    assert!(done.failures.is_empty(), "{done:?}");
    assert!(exists(&two, "/dst/src/id_rsa").await);
}
