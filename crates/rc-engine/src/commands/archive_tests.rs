use std::{path::Path, time::Duration};

use rc_ops::{Answer, ConflictChoice, JobOptions, JobState, Question};
use rc_vfs::{BackendId, UnsafeName};

use crate::test_support::expect_panel;
use crate::{
    Command, CursorTarget, Engine, EngineError, Outcome, PanelId, PanelSnapshot,
    archives::tests::{engine_beside, fixture},
    viewer::ReadTextRequest,
};

async fn run(engine: &Engine, command: Command) -> Result<Outcome, EngineError> {
    engine.dispatch(command).await
}

async fn panel(engine: &Engine, command: Command) -> PanelSnapshot {
    expect_panel(run(engine, command).await.expect("a panel"))
}

async fn cursor_on(engine: &Engine, name: &str) {
    run(
        engine,
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name(name.into()),
        },
    )
    .await
    .expect("cursor");
}

fn enter() -> Command {
    Command::OpenFile {
        panel: None,
        path: None,
    }
}

fn up() -> Command {
    Command::Navigate {
        panel: None,
        path: "..".into(),
    }
}

fn code<T: std::fmt::Debug>(result: Result<T, EngineError>) -> &'static str {
    match result {
        Ok(value) => panic!("refused, not {value:?}"),
        Err(error) => error.code(),
    }
}

#[tokio::test]
async fn enter_opens_each_archive_format_and_backspace_returns_onto_the_archive() {
    for name in [
        "tree.zip",
        "tree.7z",
        "tree.tar.gz",
        "tree.tar.xz",
        "tree.tar.bz2",
        "tree.tar.zst",
    ] {
        let (root, engine) = engine_beside(name).await;
        cursor_on(&engine, name).await;
        let inside = panel(&engine, enter()).await;
        assert!(
            matches!(inside.path.backend, BackendId::Archive { .. }),
            "{name}: {inside:?}"
        );
        let listed = engine.panel(PanelId::Left);
        assert!(listed.index_of("hello.txt").is_some(), "{name}");
        assert!(listed.archive.is_some(), "{name}: the archive's details");
        panel(
            &engine,
            Command::Navigate {
                panel: None,
                path: "docs".into(),
            },
        )
        .await;
        panel(&engine, up()).await;
        let back = panel(&engine, up()).await;
        assert_eq!(back.path.path, root.path(), "{name}");
        assert_eq!(back.cursor.map(|cursor| cursor.name).as_deref(), Some(name));
        assert!(engine.panel(PanelId::Left).archive.is_none());
    }
}

#[tokio::test]
async fn enter_goes_by_the_name_and_ctrl_page_down_by_the_content() {
    // Enter goes by the name: a zip named .bin is a file for the system (recorded, not run).
    let (root, engine) = engine_beside("tree.zip").await;
    std::fs::copy(fixture("tree.zip"), root.path().join("data.bin")).expect("copy");
    run(&engine, Command::Refresh { panel: None })
        .await
        .expect("refresh");
    cursor_on(&engine, "data.bin").await;
    let stays = panel(&engine, enter()).await;
    assert_eq!(stays.path.path, root.path());
    // Ctrl+PageDown goes by the content.
    let inside = panel(&engine, Command::OpenEntry { panel: None }).await;
    assert!(matches!(inside.path.backend, BackendId::Archive { .. }));
}

#[tokio::test]
async fn ctrl_page_down_on_a_plain_file_leaves_the_panel() {
    let (root, engine) = engine_beside("tree.zip").await;
    std::fs::write(root.path().join("notes.txt"), b"just text").expect("write");
    run(&engine, Command::Refresh { panel: None })
        .await
        .expect("refresh");
    cursor_on(&engine, "notes.txt").await;
    let stays = panel(&engine, Command::OpenEntry { panel: None }).await;
    assert_eq!(stays.path.path, root.path());
}

#[tokio::test]
async fn a_path_through_the_archive_file_opens_it_there() {
    let (root, engine) = engine_beside("tree.7z").await;
    let typed = root.path().join("tree.7z").join("docs");
    let inside = panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: typed.to_string_lossy().into_owned(),
        },
    )
    .await;
    assert_eq!(crate::shown_path(&inside.path), typed);
    assert!(engine.panel(PanelId::Left).index_of("readme.md").is_some());
}

#[tokio::test]
async fn f3_reads_a_text_file_from_a_7z_archive() {
    let (_root, engine) = engine_beside("tree.7z").await;
    cursor_on(&engine, "tree.7z").await;
    panel(&engine, enter()).await;
    let read = run(
        &engine,
        Command::ReadText(ReadTextRequest {
            path: "docs/readme.md".into(),
            panel: None,
            offset: 0,
            max_bytes: None,
            encoding: None,
        }),
    )
    .await;
    let window = match read {
        Ok(Outcome::FileText(window)) => window,
        other => panic!("a text window, not {other:?}"),
    };
    assert_eq!(window.text, "# Readme\n\nInside an archive.\n");
}

#[tokio::test]
async fn a_file_inside_an_archive_is_unpacked_read_only_for_the_system() {
    let (_root, engine) = engine_beside("tree.zip").await;
    cursor_on(&engine, "tree.zip").await;
    panel(&engine, enter()).await;
    let inside = engine.panel(PanelId::Left).path.join("hello.txt");
    let copy = engine.unpack_to_temp(&inside).await.expect("unpacked");
    assert_eq!(copy.file_name(), Some("hello.txt".as_ref()));
    assert_eq!(std::fs::read(&copy).expect("read"), b"hello archive\n");
    assert!(
        std::fs::metadata(&copy)
            .expect("stat")
            .permissions()
            .readonly()
    );
    if let Some(dir) = copy.parent() {
        // Best effort: Windows keeps a read-only file.
        let _ = std::fs::remove_dir_all(dir);
    }
    cursor_on(&engine, "hello.txt").await;
    panel(&engine, enter()).await;
}

#[tokio::test]
async fn changes_inside_an_archive_are_refused_and_f5_copies_out() {
    let (root, engine) = engine_beside("tree.zip").await;
    cursor_on(&engine, "tree.zip").await;
    panel(&engine, enter()).await;
    for command in [
        Command::OpsDelete(crate::DeleteRequest {
            panel: None,
            sources: vec!["hello.txt".into()],
            mode: rc_ops::DeleteMode::Permanent,
            on_error: rc_ops::ErrorPolicy::Skip,
            queue: None,
        }),
        Command::OpsMkdir {
            panel: None,
            path: "new".into(),
        },
        Command::OpsMove(crate::TransferRequest {
            panel: None,
            sources: vec!["hello.txt".into()],
            target: None,
            options: JobOptions::default(),
        }),
        Command::OpsCopy(crate::TransferRequest {
            panel: Some(PanelId::Right),
            sources: vec![],
            target: None,
            options: JobOptions::default(),
        }),
    ] {
        assert_eq!(code(run(&engine, command).await), "archive_read_only");
    }
    let copied = run(
        &engine,
        Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec!["hello.txt".into()],
            target: None,
            options: JobOptions::default(),
        }),
    )
    .await;
    let job = match copied {
        Ok(Outcome::Job(job)) => job,
        other => panic!("a job, not {other:?}"),
    };
    let done = engine.ops().wait(job.id).await.expect("finishes");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        std::fs::read(root.path().join("hello.txt")).expect("copied out"),
        b"hello archive\n"
    );
}

#[tokio::test]
async fn archive_list_answers_what_the_panel_shows_with_the_marks() {
    let (root, engine) = engine_beside("unsafe.zip").await;
    let listed = run(
        &engine,
        Command::ArchiveList {
            panel: None,
            path: Some("unsafe.zip".into()),
            dir: None,
        },
    )
    .await;
    let (entries, shown) = match listed {
        Ok(Outcome::ArchiveListing { entries, shown, .. }) => (entries, shown),
        other => panic!("a listing, not {other:?}"),
    };
    assert_eq!(shown, root.path().join("unsafe.zip"));
    cursor_on(&engine, "unsafe.zip").await;
    panel(&engine, enter()).await;
    let shown_panel = engine.panel(PanelId::Left);
    let names: Vec<&str> = entries
        .iter()
        .map(|item| item.entry.name.as_str())
        .collect();
    let panel_names: Vec<&str> = shown_panel
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, panel_names);
    let marks = shown_panel.archive.expect("the archive's details");
    assert_eq!(
        marks.get("evil.txt").and_then(|info| info.unsafe_name),
        Some(UnsafeName::ParentDir)
    );
    assert_eq!(
        marks.get("safe.txt").and_then(|info| info.unsafe_name),
        None
    );
}

#[tokio::test]
async fn archive_test_queues_a_job_that_passes_a_sound_archive() {
    let (_root, engine) = engine_beside("tree.tar.gz").await;
    cursor_on(&engine, "tree.tar.gz").await;
    let queued = run(
        &engine,
        Command::ArchiveTest {
            panel: None,
            paths: vec![],
        },
    )
    .await;
    let jobs = match queued {
        Ok(Outcome::Jobs { jobs }) => jobs,
        other => panic!("jobs, not {other:?}"),
    };
    assert_eq!(jobs.len(), 1);
    let done = engine.ops().wait(jobs[0].id).await.expect("finishes");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(done.failures.is_empty(), "{:?}", done.failures);
    assert!(done.progress.files_done > 0);
    assert!(Path::new(&fixture("tree.tar.gz")).exists());
}

/// The left panel open on the fixture `name`, the right one on `out/` beside it.
async fn archive_beside_out(name: &str) -> (tempfile::TempDir, Engine) {
    let (root, engine) = engine_beside(name).await;
    let out = root.path().join("out");
    std::fs::create_dir(&out).expect("mkdir");
    panel(
        &engine,
        Command::Navigate {
            panel: Some(PanelId::Right),
            path: out.to_string_lossy().into_owned(),
        },
    )
    .await;
    cursor_on(&engine, name).await;
    panel(&engine, enter()).await;
    (root, engine)
}

/// The jobs `queued` started - one per source, RC-010-14 - once they are over, in their order.
async fn finished(
    engine: &Engine,
    queued: Result<Outcome, EngineError>,
) -> Vec<rc_ops::JobSnapshot> {
    let jobs = match queued {
        Ok(Outcome::Job(job)) => vec![*job],
        Ok(Outcome::Jobs { jobs }) => jobs,
        other => panic!("jobs, not {other:?}"),
    };
    let mut done = Vec::with_capacity(jobs.len());
    for job in jobs {
        let over = tokio::time::timeout(Duration::from_secs(20), engine.ops().wait(job.id))
            .await
            .expect("in time")
            .expect("finishes");
        done.push(over);
    }
    done
}

/// The acceptance check of RC-030-03: F5 from an opened archive copies a folder with what is
/// below it - folders the archive only implies (`deep/nested`) included - by `ops.copy` and by
/// the dialog's `ops.copy_to` alike.
#[tokio::test]
async fn f5_copies_a_folder_out_of_an_archive_with_its_structure() {
    let (root, engine) = archive_beside_out("tree.7z").await;
    let out = root.path().join("out");
    let copied = run(
        &engine,
        Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec!["deep".into()],
            target: None,
            options: JobOptions::default(),
        }),
    )
    .await;
    for done in finished(&engine, copied).await {
        assert_eq!(done.state, JobState::Completed, "{done:?}");
    }
    let data = std::fs::read(out.join("deep").join("nested").join("data.bin")).expect("copied");
    assert_eq!(data.len(), 3000);
    assert!(
        data.iter()
            .enumerate()
            .all(|(at, byte)| usize::from(*byte) == at % 251)
    );

    // The F5 dialog's way: the target as typed, relative to the panel inside the archive.
    let typed = run(
        &engine,
        Command::OpsCopyTo {
            panel: None,
            sources: vec!["docs".into(), "conformance".into()],
            target: "../out".into(),
            options: JobOptions::default(),
            queue: None,
        },
    )
    .await;
    for done in finished(&engine, typed).await {
        assert_eq!(done.state, JobState::Completed, "{done:?}");
    }
    assert_eq!(
        std::fs::read(out.join("docs").join("readme.md")).expect("copied"),
        b"# Readme\n\nInside an archive.\n"
    );
    assert_eq!(
        std::fs::read(out.join("conformance").join("a.txt")).expect("copied"),
        b"hello"
    );
    // Nothing goes into an archive and nothing moves out of one, typed target or not.
    for command in [
        Command::OpsCopyTo {
            panel: None,
            sources: vec!["hello.txt".into()],
            target: "docs".into(),
            options: JobOptions::default(),
            queue: None,
        },
        Command::OpsMoveTo {
            panel: None,
            sources: vec!["hello.txt".into()],
            target: "../out".into(),
            options: JobOptions::default(),
            queue: None,
        },
    ] {
        assert_eq!(code(run(&engine, command).await), "archive_read_only");
    }
}

/// RC-030-03: a conflict while copying out of an archive is the queue's question, as for any
/// copy (the window's conflict dialog answers it); "skip all" answers every later conflict, the
/// files that were there stay, the rest is copied.
#[tokio::test]
async fn f5_out_of_an_archive_asks_and_skip_all_answers_every_conflict() {
    let (root, engine) = archive_beside_out("tree.zip").await;
    let out = root.path().join("out");
    std::fs::create_dir(out.join("docs")).expect("mkdir");
    std::fs::write(out.join("hello.txt"), b"old").expect("write");
    std::fs::write(out.join("docs").join("readme.md"), b"old").expect("write");
    let queued = run(
        &engine,
        Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec!["docs".into(), "hello.txt".into(), "empty.txt".into()],
            target: None,
            options: JobOptions::default(),
        }),
    )
    .await;
    // One job per source (RC-010-14): `docs` asks first; its siblings share the answer.
    let job = match &queued {
        Ok(Outcome::Jobs { jobs }) => jobs[0].id,
        other => panic!("one job per source, not {other:?}"),
    };
    let mut asked = None;
    for _ in 0..500 {
        asked = engine.ops().job(job).expect("known").question;
        if asked.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        matches!(asked, Some(Question::Conflict { .. })),
        "{asked:?}"
    );
    run(
        &engine,
        Command::OpsAnswer {
            job,
            answer: Answer::Conflict {
                choice: ConflictChoice::Skip,
                for_all: true,
            },
        },
    )
    .await
    .expect("answered");
    // Over without a second question: "for all" answered the other conflict, in a sibling.
    let done = finished(&engine, queued).await;
    for job in &done {
        assert_eq!(job.state, JobState::Completed, "{job:?}");
    }
    let skipped: u64 = done.iter().map(|job| job.skipped).sum();
    assert_eq!(skipped, 2, "{done:?}");
    assert_eq!(std::fs::read(out.join("hello.txt")).expect("read"), b"old");
    assert_eq!(
        std::fs::read(out.join("docs").join("readme.md")).expect("read"),
        b"old"
    );
    assert!(out.join("empty.txt").is_file(), "the rest is copied");
}
