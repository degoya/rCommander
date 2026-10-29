use std::{fs, path::Path, time::Duration};

use rc_ops::{JobKind, JobOptions, JobSpec, JobState, QueueListId};
use rc_vfs::{BackendId, VfsPath};

use crate::test_support;
use crate::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, Engine, EngineError, Outcome, SavedList,
    SecretKind, SecretParam,
};

/// A password no saved list may ever hold.
const CANARY: &str = "hunter2-queue-canary";

/// An engine on the database file in `dir` (a second one on the same file is the app started
/// anew), with a vault that derives in milliseconds; `a.txt` and `b.txt` in `dir`.
async fn engine_in(dir: &Path) -> Engine {
    fs::write(dir.join("a.txt"), b"a").expect("write");
    fs::write(dir.join("b.txt"), b"b").expect("write");
    test_support::engine_in(dir).await
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("dispatched")
}

/// A new list named `name`; its id.
async fn create(engine: &Engine, name: &str) -> QueueListId {
    let Outcome::QueueLists { lists, .. } = run(
        engine,
        Command::QueueCreate {
            name: name.into(),
            jobs: Vec::new(),
        },
    )
    .await
    else {
        panic!("the lists");
    };
    lists
        .iter()
        .find(|list| list.name == name)
        .map(|list| list.id)
        .expect("listed")
}

fn copy(source: VfsPath, target: VfsPath) -> JobSpec {
    JobSpec {
        kind: JobKind::Copy {
            sources: vec![source],
            target,
        },
        options: JobOptions::default(),
    }
}

async fn save(engine: &Engine, list: QueueListId) -> SavedList {
    let command = Command::QueueSave {
        list,
        name: None,
        restore_on_start: false,
    };
    match run(engine, command).await {
        Outcome::QueueSaved { saved, left_out } => {
            assert!(left_out.is_empty(), "{left_out:?}");
            saved
        }
        other => panic!("saved, got {other:?}"),
    }
}

/// The acceptance check: a list saved, the engine started anew on the same database, the list
/// loaded - the same entries in the same order, waiting until the list starts.
#[tokio::test]
async fn a_saved_list_comes_back_after_a_restart_waiting_in_its_order() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let specs = [
        copy(
            VfsPath::local(root.join("b.txt")),
            VfsPath::local(root.join("out")),
        ),
        copy(
            VfsPath::local(root.join("a.txt")),
            VfsPath::local(root.join("out")),
        ),
    ];
    {
        let engine = engine_in(root).await;
        let list = create(&engine, "Backup").await;
        for spec in &specs {
            let job = engine.ops().submit_into(spec.clone(), None, list);
            assert_eq!(job.expect("held").state, JobState::Queued);
        }
        let saved = save(&engine, list).await;
        assert_eq!((saved.name.as_str(), saved.entries), ("Backup", 2));
    }

    let engine = engine_in(root).await;
    let Outcome::QueueLists { saved, .. } = run(&engine, Command::QueueLists {}).await else {
        panic!("the lists");
    };
    let saved = saved.first().expect("the saved list").id;
    let Outcome::QueueLoaded {
        list,
        jobs,
        unloadable,
    } = run(&engine, Command::QueueLoad { saved }).await
    else {
        panic!("loaded");
    };
    assert!(unloadable.is_empty(), "{unloadable:?}");
    assert_eq!(list.name, "Backup");
    assert!(!list.started, "a loaded list waits");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let entries: Vec<_> = engine
        .ops()
        .entries()
        .into_iter()
        .filter(|entry| entry.list == list.id)
        .collect();
    let kinds: Vec<&JobKind> = entries.iter().map(|entry| &entry.job.kind).collect();
    assert_eq!(kinds, [&specs[0].kind, &specs[1].kind], "the saved order");
    assert!(
        entries
            .iter()
            .all(|entry| entry.job.state == JobState::Queued),
        "{entries:?}"
    );

    run(&engine, Command::QueueStart { list: list.id }).await;
    for job in jobs {
        let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
            .await
            .expect("in time")
            .expect("known");
        assert_eq!(done.state, JobState::Completed);
    }
    assert!(root.join("out").join("a.txt").exists());
}

/// The acceptance checks: no saved list holds a secret - a connection is named by its id only -
/// and an entry whose connection was deleted is named as not loadable instead of dropped.
#[tokio::test]
async fn a_saved_list_holds_no_secret_and_names_an_entry_whose_connection_went() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let engine = engine_in(root).await;
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new("web", "example.org")
    };
    let Outcome::Connection { connection } = run(
        &engine,
        Command::CreateConnection {
            folder_id: None,
            connection: draft,
        },
    )
    .await
    else {
        panic!("a connection");
    };
    let web: ConnectionId = connection.id;
    run(
        &engine,
        Command::CreateVault {
            password: SecretParam::from("correct horse battery"),
        },
    )
    .await;
    run(
        &engine,
        test_support::store_secret(web, SecretKind::Password, SecretParam::from(CANARY)),
    )
    .await;

    let list = create(&engine, "Upload").await;
    let server = VfsPath {
        backend: BackendId::Sftp {
            connection_id: web.0,
        },
        path: "/srv/www".into(),
    };
    let upload = copy(VfsPath::local(root.join("a.txt")), server);
    let local = copy(
        VfsPath::local(root.join("b.txt")),
        VfsPath::local(root.join("out")),
    );
    // Held by the stopped list: the server is never opened.
    engine.ops().load(upload, list).expect("held");
    engine.ops().load(local, list).expect("held");
    let saved = save(&engine, list).await;

    let (row, texts) = engine
        .db()
        .expect("db")
        .saved_queue(saved.id)
        .await
        .expect("read")
        .expect("there");
    let stored = format!("{row:?} {texts:?}");
    assert!(!stored.contains(CANARY), "{stored}");
    // The JSON rows themselves, not their `Debug` form (which escapes the quotes).
    assert!(
        texts
            .iter()
            .any(|text| text.contains(&format!("\"connection_id\":{}", web.0))),
        "{stored}"
    );

    run(&engine, Command::DeleteConnection { id: web }).await;
    let Outcome::QueueLoaded {
        jobs, unloadable, ..
    } = run(&engine, Command::QueueLoad { saved: saved.id }).await
    else {
        panic!("loaded");
    };
    assert_eq!(jobs.len(), 1, "the local copy loads");
    assert_eq!(unloadable.len(), 1);
    assert_eq!(
        (unloadable[0].position, unloadable[0].code.as_str()),
        (0, "connection_not_found")
    );
}

#[tokio::test]
async fn lists_are_made_named_and_refused_by_their_codes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let list = create(&engine, "Disk G").await;
    let empty = engine
        .dispatch(Command::QueueSave {
            list,
            name: None,
            restore_on_start: false,
        })
        .await
        .map(|_| ())
        .map_err(|error| error.code());
    assert_eq!(empty, Err("queue_list_empty"));
    let renamed = run(
        &engine,
        Command::QueueRename {
            list,
            name: "Disk H".into(),
        },
    )
    .await;
    let Outcome::QueueLists { lists, .. } = renamed else {
        panic!("the lists");
    };
    assert!(lists.iter().any(|found| found.name == "Disk H"));
    let refused = |command: Command| {
        let engine = engine.clone();
        async move {
            engine
                .dispatch(command)
                .await
                .map(|_| ())
                .map_err(|error| error.code())
        }
    };
    assert_eq!(
        refused(Command::QueueCreate {
            name: " ".into(),
            jobs: Vec::new(),
        })
        .await,
        Err("queue_name_invalid")
    );
    assert_eq!(
        refused(Command::QueueDelete {
            list: rc_ops::DEFAULT_LIST
        })
        .await,
        Err("queue_list_fixed")
    );
    assert_eq!(
        refused(Command::QueueLoad { saved: 77 }).await,
        Err("saved_queue_not_found")
    );
    assert_eq!(
        refused(Command::QueueDeleteSaved { saved: 77 }).await,
        Err("saved_queue_not_found")
    );
    let Outcome::QueueLists { lists, .. } = run(&engine, Command::QueueDelete { list }).await
    else {
        panic!("the lists");
    };
    assert_eq!(lists.len(), 1, "the default list stays");
}

/// A job run at once (F5 confirmed with OK) is in no list (RC-010-25): no new list takes it,
/// nothing moves it, and the jobs run at once are no list to save.
#[tokio::test]
async fn a_job_run_at_once_goes_into_no_list_and_is_never_saved() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let engine = engine_in(root).await;
    let spec = copy(
        VfsPath::local(root.join("a.txt")),
        VfsPath::local(root.join("out")),
    );
    let job = engine
        .ops()
        .submit_into(spec, None, rc_ops::AT_ONCE)
        .expect("run at once")
        .id;
    let refused = |command: Command| {
        let engine = engine.clone();
        async move {
            engine
                .dispatch(command)
                .await
                .map(|_| ())
                .map_err(|error| error.code())
        }
    };
    let create = Command::QueueCreate {
        name: "Mine".into(),
        jobs: vec![job],
    };
    assert_eq!(refused(create).await, Err("job_at_once"));
    let Outcome::QueueLists { lists, .. } = run(&engine, Command::QueueLists {}).await else {
        panic!("the lists");
    };
    assert_eq!(lists.len(), 1, "no list was made");
    let moved = Command::QueueMove {
        job,
        step: rc_ops::Reorder::Front,
    };
    assert!(matches!(
        refused(moved).await,
        Err("job_at_once" | "job_finished")
    ));
    let saved = Command::QueueSave {
        list: rc_ops::AT_ONCE,
        name: Some("Now".into()),
        restore_on_start: false,
    };
    assert_eq!(refused(saved).await, Err("queue_list_not_found"));
    let assigned = Command::QueueAssignMany {
        jobs: vec![job],
        list: rc_ops::DEFAULT_LIST,
    };
    assert_eq!(refused(assigned).await, Err("job_at_once"));
}

/// RC-080-42: loaded through MCP, an entry whose source the read guard keeps from MCP is named
/// and not loaded, the others are; the places each entry writes to are listed for the guard.
#[tokio::test]
async fn an_mcp_load_judges_each_entry() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let engine = engine_in(root).await;
    fs::create_dir_all(root.join(".ssh")).expect("mkdir");
    fs::write(root.join(".ssh").join("id_rsa"), b"key").expect("write");
    let list = create(&engine, "Keys").await;
    let out = VfsPath::local(root.join("out"));
    let key = copy(VfsPath::local(root.join(".ssh")), out.clone());
    let plain = copy(VfsPath::local(root.join("a.txt")), out.clone());
    engine.ops().load(key, list).expect("held");
    engine.ops().load(plain, list).expect("held");
    let saved = save(&engine, list).await;

    let places = engine
        .saved_queue_places(saved.id)
        .await
        .expect("the places");
    assert_eq!(
        places,
        [
            (VfsPath::local(root.join(".ssh")), out.join(".ssh")),
            (VfsPath::local(root.join("a.txt")), out.join("a.txt")),
        ]
    );
    let judge: crate::read_guard::ReadJudge = |path| {
        path.components()
            .any(|part| part.as_os_str() == ".ssh")
            .then_some("an SSH folder")
    };
    let Outcome::QueueLoaded {
        jobs, unloadable, ..
    } = engine
        .dispatch_judged(Command::QueueLoad { saved: saved.id }, Some(judge))
        .await
        .expect("loaded")
    else {
        panic!("loaded");
    };
    assert_eq!(jobs.len(), 1, "the plain copy loads");
    assert_eq!(
        unloadable
            .iter()
            .map(|entry| (entry.position, entry.code.as_str()))
            .collect::<Vec<_>>(),
        [(0, "sensitive_path")]
    );
    assert!(matches!(
        engine.saved_queue_places(9_999).await,
        Err(EngineError::SavedQueueNotFound { saved: 9_999 })
    ));
}
