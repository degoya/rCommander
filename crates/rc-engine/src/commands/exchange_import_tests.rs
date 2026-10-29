//! `config.import` (RC-080-19): an export restores every part in a fresh database and a second
//! import adds nothing; a wrong export password, a changed byte, a file too large or not an
//! export changes nothing; a host key that differs from the trusted one is a conflict that is
//! replaced only when ticked; secrets wait for the vault and land only there.

use rc_vault::ExposeSecret as _;

use crate::{
    Command, ConnectionKind, Engine, Outcome, SecretKind, SecretRef,
    commands::exchange_fixture::{
        CANARY_PASSWORD, CANARY_SCRIPT, EXPORT_PASSWORD, HOST_KEY, OTHER_HOST_KEY, create_vault,
        engine_in, export, import, report, seeded, trust_key,
    },
    exchange::{ExchangePart, ExportParts, MAX_FILE_BYTES, RowStatus},
    settings::{THEME_KEY, stored_setting},
};

/// The connections of `engine`'s tree, root and folders, by name.
async fn names(engine: &Engine) -> Vec<String> {
    let tree = engine
        .db()
        .expect("db")
        .connection_tree()
        .await
        .expect("tree");
    let mut names: Vec<String> = tree.connections.iter().map(|c| c.name.clone()).collect();
    for folder in &tree.folders {
        names.extend(folder.connections.iter().map(|c| c.name.clone()));
    }
    names.sort();
    names
}

/// A sealed export with secrets of a seeded engine, in `dir`.
async fn sealed_export(dir: &std::path::Path) -> std::path::PathBuf {
    let (engine, _) = seeded(dir).await;
    let path = dir.join("all.rcexport");
    engine
        .dispatch(export(&path, ExportParts::ALL, Some(EXPORT_PASSWORD), true))
        .await
        .expect("exported");
    path
}

#[tokio::test]
async fn a_fresh_database_gets_every_part_and_a_second_import_adds_nothing() {
    let source = tempfile::tempdir().expect("dir");
    let path = sealed_export(source.path()).await;
    let target = tempfile::tempdir().expect("dir");
    let engine = engine_in(target.path()).await;
    create_vault(&engine).await;

    let preview = report(&engine, import(&path, Some(EXPORT_PASSWORD), true, None)).await;
    assert!(preview.encrypted && preview.secrets);
    assert!(names(&engine).await.is_empty(), "a preview writes nothing");
    let new = |part| {
        preview
            .rows
            .iter()
            .filter(|row| row.part == part && row.status == RowStatus::New)
            .count()
    };
    assert_eq!(new(ExchangePart::Connections), 3, "{:?}", preview.rows);
    assert_eq!(new(ExchangePart::Trust), 2);

    let done = report(&engine, import(&path, Some(EXPORT_PASSWORD), false, None)).await;
    assert!(
        done.rows
            .iter()
            .filter(|row| row.selected)
            .all(|row| matches!(row.status, RowStatus::Created | RowStatus::Updated)),
        "{:?}",
        done.rows
    );
    assert_eq!(names(&engine).await, ["nas", "web"]);
    let tree = engine
        .db()
        .expect("db")
        .connection_tree()
        .await
        .expect("tree");
    assert_eq!(tree.folders[0].folder.name, "prod");
    let web = &tree.folders[0].connections[0];
    assert!(web.login_script_id.is_some());
    assert_eq!(tree.connections[0].kind, ConnectionKind::Ftp);
    let password = engine
        .resolve_secret(SecretRef {
            connection: web.id,
            kind: SecretKind::Password,
        })
        .await
        .expect("read")
        .expect("stored");
    assert_eq!(password.expose_secret(), CANARY_PASSWORD);
    let Ok(Outcome::LoginScript { script }) = engine
        .dispatch(Command::GetLoginScript {
            id: crate::LoginScriptId(web.login_script_id.expect("a script")),
        })
        .await
    else {
        panic!("a script");
    };
    let secret_id = script.secrets[0].id;
    assert_eq!(
        script.steps,
        crate::commands::exchange_fixture::sudo_steps(Some(secret_id)),
        "the step names the secret by its id here"
    );
    let value = engine
        .resolve_script_secret(script.id, secret_id)
        .await
        .expect("read")
        .expect("stored");
    assert_eq!(value.expose_secret(), CANARY_SCRIPT);
    let db = engine.db().expect("db");
    assert_eq!(db.known_hosts().await.expect("keys").len(), 1);
    assert_eq!(db.trusted_certificates().await.expect("certs").len(), 1);
    assert_eq!(db.hotlist().await.expect("hotlist").len(), 2);
    assert_eq!(
        stored_setting(db, THEME_KEY).await,
        Some(serde_json::json!("dark"))
    );

    let again = report(&engine, import(&path, Some(EXPORT_PASSWORD), false, None)).await;
    assert!(
        again
            .rows
            .iter()
            .all(|row| !matches!(row.status, RowStatus::Created | RowStatus::New)),
        "{:?}",
        again.rows
    );
    assert_eq!(names(&engine).await, ["nas", "web"]);
    assert_eq!(db.hotlist().await.expect("hotlist").len(), 2);
}

#[tokio::test]
async fn a_wrong_or_missing_password_changes_nothing() {
    let source = tempfile::tempdir().expect("dir");
    let path = sealed_export(source.path()).await;
    let target = tempfile::tempdir().expect("dir");
    let engine = engine_in(target.path()).await;
    create_vault(&engine).await;
    for (password, code) in [
        (Some("wrong password"), "exchange_password_wrong"),
        (None, "exchange_password_needed"),
        (Some(""), "exchange_password_needed"),
    ] {
        let refused = engine
            .dispatch(import(&path, password, false, None))
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), code);
    }
    assert!(names(&engine).await.is_empty());
    assert!(
        engine
            .db()
            .expect("db")
            .hotlist()
            .await
            .expect("read")
            .is_empty()
    );
}

#[tokio::test]
async fn a_changed_damaged_or_oversized_file_changes_nothing() {
    let source = tempfile::tempdir().expect("dir");
    let path = sealed_export(source.path()).await;
    let target = tempfile::tempdir().expect("dir");
    let engine = engine_in(target.path()).await;
    create_vault(&engine).await;
    let text = std::fs::read_to_string(&path).expect("read");
    let mut file: serde_json::Value = serde_json::from_str(&text).expect("json");
    let content = file["sealed"]["content"].as_str().expect("hex").to_owned();
    let flipped = if content.ends_with('0') { "1" } else { "0" };
    file["sealed"]["content"] =
        serde_json::Value::String(format!("{}{flipped}", &content[..content.len() - 1]));
    let changed = target.path().join("changed.rcexport");
    std::fs::write(&changed, file.to_string()).expect("written");
    let garbage = target.path().join("garbage.rcexport");
    std::fs::write(&garbage, "{\"format\":\"rcommander-export\"").expect("written");
    let huge = target.path().join("huge.rcexport");
    let file = std::fs::File::create(&huge).expect("created");
    file.set_len(MAX_FILE_BYTES + 1).expect("sized");
    for (file, code) in [
        (&changed, "exchange_password_wrong"),
        (&garbage, "exchange_file_invalid"),
        (&huge, "exchange_file_too_large"),
    ] {
        let refused = engine
            .dispatch(import(file, Some(EXPORT_PASSWORD), false, None))
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), code, "{}", file.display());
    }
    assert!(names(&engine).await.is_empty());
}

#[tokio::test]
async fn a_bad_row_is_refused_alone_and_no_value_reaches_an_error() {
    let target = tempfile::tempdir().expect("dir");
    let engine = engine_in(target.path()).await;
    let path = target.path().join("rows.rcexport");
    let content = serde_json::json!({
        "origin": "abc",
        "secrets": false,
        "settings": [
            {"setting": "appearance.theme", "value": "dark"},
            {"setting": "appearance.theme_of_mars", "value": true},
            {"setting": "mcp.http.enabled", "value": false}
        ],
        "trust": {
            "host_keys": [
                {"host": "box", "port": 22, "algorithm": "ssh-ed25519", "public_key": "AAAA"},
                {"host": "ok.example.org", "port": 22, "algorithm": "ssh-ed25519",
                 "public_key": HOST_KEY}
            ],
            "certificates": []
        }
    });
    let file = serde_json::json!({
        "format": "rcommander-export", "version": 1, "created_at": "", "content": content
    });
    std::fs::write(&path, file.to_string()).expect("written");
    let done = report(&engine, import(&path, None, false, None)).await;
    let status = |key: &str| {
        done.rows
            .iter()
            .find(|row| row.key == key)
            .map(|row| (row.status.clone(), row.warning.clone()))
            .expect("a row")
    };
    assert_eq!(status("setting:appearance.theme").0, RowStatus::Updated);
    assert!(matches!(
        status("setting:appearance.theme_of_mars").0,
        RowStatus::Refused { .. }
    ));
    let mcp = status("setting:mcp.http.enabled");
    assert_eq!(mcp.0, RowStatus::Changed, "protected: not written unticked");
    assert_eq!(mcp.1.as_deref(), Some("protected_setting"));
    assert!(matches!(
        status("host_key:box:22:ssh-ed25519").0,
        RowStatus::Refused { .. }
    ));
    assert_eq!(
        status("host_key:ok.example.org:22:ssh-ed25519").0,
        RowStatus::Created
    );

    let leaking = r#"{"format":"rcommander-export","version":1,"created_at":"","content":{"origin":"abc","secrets":true,"connections":{"connections":[{"source":"rcexport:abc","key":"1","connection":{"name":"x","host":"h","port":"canary-hunter2"}}],"login_scripts":[]}}}"#;
    std::fs::write(&path, leaking).expect("written");
    let refused = engine
        .dispatch(import(&path, None, true, None))
        .await
        .expect_err("not a port");
    assert_eq!(refused.code(), "exchange_file_invalid");
    assert!(!refused.to_string().contains("canary-hunter2"), "{refused}");
}

#[tokio::test]
async fn a_changed_host_key_is_a_conflict_replaced_only_when_ticked() {
    let source = tempfile::tempdir().expect("dir");
    let (from, _) = seeded(source.path()).await;
    let path = source.path().join("trust.rcexport");
    let trust = ExportParts {
        trust: true,
        ..ExportParts::default()
    };
    from.dispatch(export(&path, trust, None, false))
        .await
        .expect("exported");
    let target = tempfile::tempdir().expect("dir");
    let engine = engine_in(target.path()).await;
    let db = engine.db().expect("db").clone();
    trust_key(&db, "web.example.org", OTHER_HOST_KEY).await;
    let key = "host_key:web.example.org:22:ssh-ed25519".to_owned();

    let preview = report(&engine, import(&path, None, true, None)).await;
    let row = preview.rows.iter().find(|row| row.key == key).expect("row");
    assert_eq!(row.status, RowStatus::Changed);
    assert_eq!(row.warning.as_deref(), Some("host_key_changed"));
    assert!(!row.selected, "a conflict is never chosen by default");

    report(&engine, import(&path, None, false, None)).await;
    let kept = db.known_hosts().await.expect("keys");
    assert_eq!(kept.len(), 1);
    assert!(
        kept[0].public_key.ends_with("Hh8g"),
        "kept: {}",
        kept[0].public_key
    );

    let ticked = report(&engine, import(&path, None, false, Some(vec![key.clone()]))).await;
    let row = ticked.rows.iter().find(|row| row.key == key).expect("row");
    assert_eq!(row.status, RowStatus::Updated);
    let replaced = db.known_hosts().await.expect("keys");
    assert_eq!(replaced[0].public_key, HOST_KEY);
}

#[tokio::test]
async fn secrets_wait_for_the_vault_and_nothing_is_written_before() {
    let source = tempfile::tempdir().expect("dir");
    let path = sealed_export(source.path()).await;
    let target = tempfile::tempdir().expect("dir");
    let engine = engine_in(target.path()).await;
    let absent = engine
        .dispatch(import(&path, Some(EXPORT_PASSWORD), false, None))
        .await
        .expect_err("no vault");
    assert_eq!(absent.code(), "vault_absent");
    assert!(names(&engine).await.is_empty());
    assert!(
        engine
            .db()
            .expect("db")
            .known_hosts()
            .await
            .expect("keys")
            .is_empty()
    );
    // A preview needs no vault.
    report(&engine, import(&path, Some(EXPORT_PASSWORD), true, None)).await;
}
