use crate::{
    Db,
    connections::{
        ConnectionDraft, ConnectionError, ConnectionTree, FolderId, ImportCandidate, ImportResult,
        ImportStatus, ImportTarget,
    },
};

const SOURCE: &str = "ssh_config:/home/me/.ssh/config";

fn candidate(alias: &str, host: &str) -> ImportCandidate {
    ImportCandidate {
        key: alias.to_ascii_lowercase(),
        draft: ConnectionDraft::new(alias, host),
        selected: true,
    }
}

fn imported_folder() -> ImportTarget {
    ImportTarget::RootFolderNamed("Imported from ssh config".into())
}

async fn import(db: &Db, candidates: &[ImportCandidate], dry_run: bool) -> ImportResult {
    db.import_connections(SOURCE, &imported_folder(), candidates, dry_run)
        .await
        .expect("import")
}

fn count(tree: &ConnectionTree) -> usize {
    tree.connections.len()
        + tree
            .folders
            .iter()
            .map(|folder| folder.connections.len())
            .sum::<usize>()
}

#[tokio::test]
async fn a_dry_run_plans_and_writes_nothing() {
    let db = Db::open_in_memory().await.expect("open");
    let candidates = [
        candidate("web", "web.example.org"),
        candidate("db", "10.0.0.5"),
    ];
    let planned = import(&db, &candidates, true).await;
    assert_eq!(planned.folder, None, "the named folder does not exist yet");
    assert!(!planned.folder_created);
    assert_eq!(planned.statuses, [ImportStatus::New, ImportStatus::New]);
    assert_eq!(
        db.connection_tree().await.expect("tree"),
        ConnectionTree::default()
    );
}

#[tokio::test]
async fn an_import_creates_the_folder_and_a_second_import_adds_no_duplicate() {
    let db = Db::open_in_memory().await.expect("open");
    let candidates = [
        candidate("web", "web.example.org"),
        candidate("db", "10.0.0.5"),
    ];
    let first = import(&db, &candidates, false).await;
    assert!(first.folder_created);
    let folder = first.folder.expect("the folder was created");
    let ids: Vec<_> = first
        .statuses
        .iter()
        .map(|status| match status {
            ImportStatus::Created { id } => *id,
            other => panic!("created, got {other:?}"),
        })
        .collect();
    let web = db.connection(ids[0]).await.expect("web");
    assert_eq!(web.folder_id, Some(folder));
    assert_eq!(web.host, "web.example.org");

    let second = import(&db, &candidates, false).await;
    assert_eq!(second.folder, Some(folder));
    assert!(!second.folder_created);
    assert_eq!(
        second.statuses,
        [
            ImportStatus::Unchanged { id: ids[0] },
            ImportStatus::Unchanged { id: ids[1] }
        ]
    );
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(tree.folders.len(), 1);
    assert_eq!(count(&tree), 2);
}

#[tokio::test]
async fn a_changed_source_updates_its_fields_and_keeps_the_persons() {
    let db = Db::open_in_memory().await.expect("open");
    let first = import(&db, &[candidate("web", "web.example.org")], false).await;
    let ImportStatus::Created { id } = first.statuses[0] else {
        panic!("created, got {:?}", first.statuses[0]);
    };
    // The person renames it and moves it to the root; the import still knows it by its key.
    db.update_connection(
        id,
        &crate::connections::ConnectionPatch {
            name: Some("Web server".into()),
            notes: Some("mine".into()),
            ..Default::default()
        },
    )
    .await
    .expect("rename");
    db.move_connection(id, None, None).await.expect("move");

    let mut changed = candidate("web", "web2.example.org");
    changed.draft.port = Some(2222);
    changed.draft.username = Some("deploy".into());
    changed.draft.key_path = Some("/home/me/.ssh/id_ed25519".into());
    let planned = import(&db, std::slice::from_ref(&changed), true).await;
    assert_eq!(planned.statuses, [ImportStatus::Update { id }]);
    assert_eq!(
        db.connection(id).await.expect("get").host,
        "web.example.org"
    );

    let done = import(&db, &[changed], false).await;
    assert_eq!(done.statuses, [ImportStatus::Updated { id }]);
    let web = db.connection(id).await.expect("get");
    assert_eq!(web.host, "web2.example.org");
    assert_eq!(web.port, 2222);
    assert_eq!(web.username.as_deref(), Some("deploy"));
    assert_eq!(web.key_path.as_deref(), Some("/home/me/.ssh/id_ed25519"));
    assert_eq!(web.name, "Web server");
    assert_eq!(web.notes.as_deref(), Some("mine"));
    assert_eq!(web.folder_id, None);
}

#[tokio::test]
async fn unselected_candidates_are_planned_but_not_written() {
    let db = Db::open_in_memory().await.expect("open");
    let mut skipped = candidate("db", "10.0.0.5");
    skipped.selected = false;
    let done = import(&db, &[candidate("web", "web.example.org"), skipped], false).await;
    assert!(matches!(done.statuses[0], ImportStatus::Created { .. }));
    assert_eq!(done.statuses[1], ImportStatus::New);
    assert_eq!(count(&db.connection_tree().await.expect("tree")), 1);
}

#[tokio::test]
async fn nothing_selected_creates_no_folder() {
    let db = Db::open_in_memory().await.expect("open");
    let mut web = candidate("web", "web.example.org");
    web.selected = false;
    let done = import(&db, &[web], false).await;
    assert_eq!(done.folder, None);
    assert!(!done.folder_created);
    assert!(db.connection_tree().await.expect("tree").folders.is_empty());
}

#[tokio::test]
async fn a_name_taken_by_a_hand_made_connection_is_refused_not_overwritten() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "servers").await.expect("folder");
    let mine = db
        .create_connection(
            Some(folder.id),
            &ConnectionDraft::new("WEB", "mine.example.org"),
        )
        .await
        .expect("mine");
    let done = db
        .import_connections(
            SOURCE,
            &ImportTarget::Folder(folder.id),
            &[
                candidate("web", "web.example.org"),
                candidate("db", "10.0.0.5"),
            ],
            false,
        )
        .await
        .expect("import");
    assert_eq!(
        done.statuses[0],
        ImportStatus::Refused {
            code: "name_taken".into()
        }
    );
    assert!(matches!(done.statuses[1], ImportStatus::Created { .. }));
    assert_eq!(
        db.connection(mine.id).await.expect("mine").host,
        "mine.example.org"
    );
}

#[tokio::test]
async fn an_invalid_candidate_is_refused_with_its_code_and_the_rest_imports() {
    let db = Db::open_in_memory().await.expect("open");
    let mut bad = candidate("bad", "bad.example.org");
    bad.draft.port = Some(0);
    let done = import(&db, &[bad, candidate("web", "web.example.org")], false).await;
    assert_eq!(
        done.statuses[0],
        ImportStatus::Refused {
            code: "port_invalid".into()
        }
    );
    assert!(matches!(done.statuses[1], ImportStatus::Created { .. }));
}

#[tokio::test]
async fn a_key_twice_in_one_import_is_refused_the_second_time() {
    let db = Db::open_in_memory().await.expect("open");
    let done = import(
        &db,
        &[
            candidate("web", "a.example.org"),
            candidate("web", "b.example.org"),
        ],
        false,
    )
    .await;
    assert!(matches!(done.statuses[0], ImportStatus::Created { .. }));
    assert_eq!(
        done.statuses[1],
        ImportStatus::Refused {
            code: "name_taken".into()
        }
    );
}

#[tokio::test]
async fn another_source_is_another_set_of_keys() {
    let db = Db::open_in_memory().await.expect("open");
    import(&db, &[candidate("web", "web.example.org")], false).await;
    let other = db
        .import_connections(
            "ssh_config:/mnt/wsl/home/me/.ssh/config",
            &ImportTarget::RootFolderNamed("From WSL".into()),
            &[candidate("web", "web.example.org")],
            false,
        )
        .await
        .expect("import");
    assert!(other.folder_created);
    assert!(matches!(other.statuses[0], ImportStatus::Created { .. }));
    assert_eq!(count(&db.connection_tree().await.expect("tree")), 2);
}

#[tokio::test]
async fn the_named_folder_is_found_ignoring_case() {
    let db = Db::open_in_memory().await.expect("open");
    let existing = db
        .create_folder(None, "imported FROM ssh config")
        .await
        .expect("folder");
    let planned = import(&db, &[candidate("web", "web.example.org")], true).await;
    assert_eq!(planned.folder, Some(existing.id));
}

#[tokio::test]
async fn a_missing_target_folder_fails_the_whole_import() {
    let db = Db::open_in_memory().await.expect("open");
    let error = db
        .import_connections(
            SOURCE,
            &ImportTarget::Folder(FolderId(99)),
            &[candidate("web", "web.example.org")],
            true,
        )
        .await
        .expect_err("refused");
    assert!(matches!(error, ConnectionError::FolderNotFound { .. }));
}

#[tokio::test]
async fn a_root_path_finds_or_creates_each_folder_on_the_way() {
    let db = Db::open_in_memory().await.expect("open");
    let top = db.create_folder(None, "Imported").await.expect("top");
    let path = ImportTarget::RootPath(vec!["imported".into(), "Work".into(), "Servers".into()]);
    let candidates = [candidate("box", "box.example.org")];
    let planned = db
        .import_connections("tc:wcx_ftp.ini", &path, &candidates, true)
        .await
        .expect("plan");
    assert_eq!(planned.folder, None, "Work does not exist yet");
    assert_eq!(planned.statuses, [ImportStatus::New]);

    let done = db
        .import_connections("tc:wcx_ftp.ini", &path, &candidates, false)
        .await
        .expect("import");
    assert!(done.folder_created);
    let servers = done.folder.expect("created");
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(tree.folders.len(), 1, "the existing top folder is reused");
    let work = &tree.folders[0].folders[0];
    assert_eq!(tree.folders[0].folder.id, top.id);
    assert_eq!(work.folder.name, "Work");
    assert_eq!(work.folders[0].folder.id, servers);
    assert_eq!(work.folders[0].connections[0].name, "box");

    let again = db
        .import_connections("tc:wcx_ftp.ini", &path, &candidates, false)
        .await
        .expect("again");
    assert_eq!(again.folder, Some(servers));
    assert!(!again.folder_created);
    assert!(matches!(again.statuses[0], ImportStatus::Unchanged { .. }));
}
