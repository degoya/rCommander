use std::{fs, path::MAIN_SEPARATOR, sync::Arc, time::Duration};

use rc_ops::{
    AT_ONCE, Answer, Backend, ConflictChoice, JobId, JobKind, JobOptions, JobSnapshot, JobState,
};
use rc_vfs::{ArchiveFormat, MemoryFs, Vfs, VfsPath};

use super::super::file_ops::typed_target;
use super::*;
use crate::test_support::{self, expect_jobs};
use crate::{Command, PanelId};

/// `root/` with `a.txt`, `b.txt`, `c.md`, `ab.txt` and `out/`; the left panel on `root`.
async fn engine() -> (tempfile::TempDir, Engine) {
    let root = test_support::tree(&[
        ("out/", ""),
        ("a.txt", "a"),
        ("b.txt", "b"),
        ("c.md", "c"),
        ("ab.txt", "ab"),
    ]);
    let engine = test_support::open(root.path(), &root.path().join("out")).await;
    (root, engine)
}

fn transfer(sources: &[&str], target: &str, moving: bool) -> Command {
    let sources = sources.iter().map(|name| (*name).to_owned()).collect();
    let (panel, target, options) = (
        Some(PanelId::Left),
        target.to_owned(),
        JobOptions::default(),
    );
    if moving {
        Command::OpsMoveTo {
            panel,
            sources,
            target,
            options,
            queue: None,
        }
    } else {
        Command::OpsCopyTo {
            panel,
            sources,
            target,
            options,
            queue: None,
        }
    }
}

fn code(result: Result<Outcome, EngineError>) -> &'static str {
    match result {
        Ok(outcome) => panic!("refused, not {outcome:?}"),
        Err(error) => error.code(),
    }
}

async fn wait(engine: &Engine, job: JobId) -> JobSnapshot {
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job))
        .await
        .expect("in time")
        .expect("known")
}

fn targets(jobs: &[JobSnapshot]) -> Vec<(String, String)> {
    let name = |path: &VfsPath| {
        path.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    jobs.iter()
        .map(|job| match &job.kind {
            JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
                (name(source), name(target))
            }
            other => panic!("a job under a new name, not {other:?}"),
        })
        .collect()
}

#[test]
fn the_last_part_of_a_typed_target_is_its_mask() {
    let sep = MAIN_SEPARATOR;
    let typed = format!("out{sep}*.bak");
    assert_eq!(
        split_mask(&typed).expect("valid"),
        (&*format!("out{sep}"), Some("*.bak"))
    );
    assert_eq!(split_mask("*.bak").expect("valid"), ("", Some("*.bak")));
    assert_eq!(split_mask("copy.txt").expect("valid"), ("copy.txt", None));
    // `out\*.*` stays "into `out`" (`typed_target` takes the `*.*` off first).
    let typed_into = format!("out{sep}*.*");
    let (folder, into) = typed_target(&typed_into);
    assert!(into);
    assert_eq!(split_mask(folder).expect("valid"), (folder, None));
    // A long path's `\\?\` is no wildcard.
    assert_eq!(
        split_mask("//?/c/x/*.bak").expect("valid"),
        ("//?/c/x/", Some("*.bak"))
    );
}

#[test]
fn a_wildcard_in_the_folder_is_an_invalid_name() {
    let sep = MAIN_SEPARATOR;
    for typed in [format!("o*t{sep}*.bak"), format!("o?t{sep}a.txt")] {
        let error = split_mask(&typed).expect_err("refused");
        assert_eq!(error.code(), "invalid_name", "{typed}");
    }
}

/// Three files with `*.bak`: three `CopyAs` with the new names, one batch.
#[tokio::test]
async fn star_dot_bak_copies_each_file_under_its_new_name_in_one_batch() {
    let (root, engine) = engine().await;
    let target = format!("out{MAIN_SEPARATOR}*.bak");
    let accepted = engine
        .dispatch(transfer(&["a.txt", "b.txt", "c.md"], &target, false))
        .await
        .expect("accepted");
    let jobs = expect_jobs(accepted);
    assert_eq!(
        targets(&jobs),
        [("a.txt", "a.bak"), ("b.txt", "b.bak"), ("c.md", "c.bak")]
            .map(|(old, new)| (old.to_owned(), new.to_owned()))
    );
    let batch = jobs[0].batch;
    assert!(batch.is_some(), "one operation");
    assert!(jobs.iter().all(|job| job.batch == batch));
    for job in &jobs {
        assert_eq!(wait(&engine, job.id).await.state, JobState::Completed);
    }
    let out = root.path().join("out");
    assert_eq!(fs::read(out.join("a.bak")).expect("copied"), b"a");
    assert_eq!(fs::read(out.join("c.bak")).expect("copied"), b"c");
    assert!(
        root.path().join("a.txt").exists(),
        "a copy keeps its source"
    );
}

#[tokio::test]
async fn two_sources_on_one_name_are_refused_before_any_job() {
    let (_root, engine) = engine().await;
    let target = format!("out{MAIN_SEPARATOR}x.*");
    let refused = engine
        .dispatch(transfer(&["a.txt", "b.txt"], &target, false))
        .await;
    let Err(error) = refused else {
        panic!("refused");
    };
    assert_eq!(error.code(), "mask_collision");
    let message = error.to_string();
    assert!(
        message.contains("a.txt") && message.contains("b.txt"),
        "{message}"
    );
    assert!(engine.ops().jobs().is_empty(), "nothing submitted");
}

/// Shift+F5 with a mask that keeps the name copies the file onto itself: refused.
#[tokio::test]
async fn a_copy_here_that_keeps_the_name_is_refused() {
    let (_root, engine) = engine().await;
    let refused = engine
        .dispatch(transfer(&["ab.txt"], "a?.txt", false))
        .await;
    assert_eq!(code(refused), "mask_unchanged");
    assert!(engine.ops().jobs().is_empty());
}

#[tokio::test]
async fn a_rename_onto_the_old_name_of_another_is_a_chain() {
    let (root, engine) = engine().await;
    fs::write(root.path().join("a1.txt"), b"1").expect("write");
    fs::write(root.path().join("b1.txt"), b"2").expect("write");
    let refused = engine
        .dispatch(transfer(&["a1.txt", "b1.txt"], "b*.txt", true))
        .await;
    assert_eq!(code(refused), "mask_chain");
    assert!(engine.ops().jobs().is_empty());
}

#[tokio::test]
async fn a_new_name_windows_refuses_is_an_invalid_name_on_every_backend() {
    let (_root, engine) = engine().await;
    let target = format!("out{MAIN_SEPARATOR}*:x.txt");
    let refused = engine.dispatch(transfer(&["a.txt"], &target, false)).await;
    assert_eq!(code(refused), "invalid_name");
    let folder = format!("o*t{MAIN_SEPARATOR}*.bak");
    let refused = engine.dispatch(transfer(&["a.txt"], &folder, false)).await;
    assert_eq!(code(refused), "invalid_name");
    assert!(engine.ops().jobs().is_empty());
}

#[test]
fn a_mask_into_an_archive_is_refused() {
    let archive = VfsPath::archive(VfsPath::local("/data/x.zip"), ArchiveFormat::Zip, "/");
    let refused = refuse_mask_into_archive(&archive).map_err(|error| error.code());
    assert_eq!(refused, Err("mask_into_archive"));
    assert!(refuse_mask_into_archive(&VfsPath::local("/data")).is_ok());
}

/// F6 with `*.old`: three files moved under their new names; the existing targets ask once, and
/// "Overwrite all" holds for the whole batch.
#[tokio::test]
async fn a_rename_mask_moves_and_one_overwrite_all_holds_for_the_batch() {
    let (root, engine) = engine().await;
    for name in ["a.old", "b.old"] {
        fs::write(root.path().join(name), b"old").expect("write");
    }
    let accepted = engine
        .dispatch(transfer(&["a.txt", "b.txt", "c.md"], "*.old", true))
        .await
        .expect("accepted");
    let jobs = expect_jobs(accepted);
    assert_eq!(jobs.len(), 3);
    let first = jobs[0].id;
    let mut asked = false;
    for _ in 0..500 {
        if engine.ops().job(first).expect("known").question.is_some() {
            asked = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(asked, "the existing a.old asks");
    let overwrite_all = Answer::Conflict {
        choice: ConflictChoice::Overwrite,
        for_all: true,
    };
    engine.ops().answer(first, overwrite_all).expect("answered");
    for job in &jobs {
        assert_eq!(wait(&engine, job.id).await.state, JobState::Completed);
    }
    for (name, text) in [("a.old", "a"), ("b.old", "b"), ("c.old", "c")] {
        assert_eq!(
            fs::read(root.path().join(name)).expect("moved"),
            text.as_bytes()
        );
    }
    assert!(!root.path().join("a.txt").exists() && !root.path().join("c.md").exists());
}

/// A target on another backend - a server - gets the names of the mask.
#[tokio::test]
async fn a_target_on_another_backend_gets_the_masked_names() {
    let (root, engine) = engine().await;
    let server = BackendId::Memory(2);
    let memory = MemoryFs::new();
    memory
        .mkdir(std::path::Path::new("/in"), true)
        .await
        .expect("mkdir");
    let vfs: Arc<dyn Vfs> = Arc::new(memory);
    engine
        .inner
        .remotes
        .open()
        .insert(server.clone(), Arc::clone(&vfs));
    engine
        .ops()
        .register(server.clone(), Backend::new(Arc::clone(&vfs)));
    let target = VfsPath {
        backend: server,
        path: "/in".into(),
    };
    let sources = ["a.txt", "b.txt"].map(|name| VfsPath::local(root.path().join(name)));
    let outcome = engine
        .submit_masked(
            sources.to_vec(),
            &target,
            "*.bak",
            (JobOptions::default(), AT_ONCE),
            false,
        )
        .await
        .expect("accepted");
    for job in expect_jobs(outcome) {
        assert_eq!(wait(&engine, job.id).await.state, JobState::Completed);
    }
    for name in ["/in/a.bak", "/in/b.bak"] {
        assert!(vfs.stat(std::path::Path::new(name)).await.is_ok(), "{name}");
    }
}

/// `ops.copy` with `rename_mask` (MCP's `ops_copy`) renames into the other panel's folder; a
/// mask that is more than a name is refused.
#[tokio::test]
async fn ops_copy_takes_a_rename_mask() {
    let (root, engine) = engine().await;
    let request = |mask: &str| crate::TransferRequest {
        panel: Some(PanelId::Left),
        sources: vec!["a.txt".into(), "b.txt".into()],
        target: None,
        options: JobOptions::default(),
        rename_mask: Some(mask.to_owned()),
    };
    let accepted = engine
        .dispatch(Command::OpsCopy(request("*.bak")))
        .await
        .expect("accepted");
    for job in expect_jobs(accepted) {
        assert_eq!(wait(&engine, job.id).await.state, JobState::Completed);
    }
    assert!(root.path().join("out").join("a.bak").is_file());
    assert!(root.path().join("out").join("b.bak").is_file());
    for mask in ["../*.bak", "x\\*.bak", " "] {
        let refused = engine.dispatch(Command::OpsCopy(request(mask))).await;
        assert_eq!(code(refused), "invalid_name", "{mask:?}");
    }
    let collision = engine.dispatch(Command::OpsCopy(request("x.*"))).await;
    assert_eq!(code(collision), "mask_collision");
}
