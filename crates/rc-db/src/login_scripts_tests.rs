use super::*;
use crate::{DB_FILE, SECRET_COLUMNS, connections::ConnectionDraft};

const STEPS: &str = r#"[{"step":"send_text","text":"sudo -i","newline":true}]"#;

fn fields(name: &str) -> LoginScriptFields<'_> {
    LoginScriptFields {
        name,
        schema_version: 1,
        steps: STEPS,
        step_timeout_ms: 15_000,
        total_timeout_ms: 120_000,
    }
}

#[tokio::test]
async fn a_script_is_created_read_listed_and_updated() {
    let db = Db::open_in_memory().await.expect("open");
    let created = db
        .create_login_script(&fields(" root shell "))
        .await
        .expect("create");
    assert_eq!(created.name, "root shell");
    assert_eq!(created.steps, STEPS);
    assert_eq!(created.step_timeout_ms, 15_000);
    assert!(created.secrets.is_empty() && created.connections.is_empty());
    assert_eq!(db.login_script(created.id).await.expect("get"), created);

    db.create_login_script(&fields("Another"))
        .await
        .expect("create");
    let names: Vec<String> = db
        .login_scripts()
        .await
        .expect("list")
        .into_iter()
        .map(|script| script.name)
        .collect();
    assert_eq!(names, ["Another", "root shell"]);

    let changed = db
        .update_login_script(
            created.id,
            &LoginScriptFields {
                steps: "[]",
                total_timeout_ms: 5_000,
                ..fields("root")
            },
        )
        .await
        .expect("update");
    assert_eq!(
        (
            changed.name.as_str(),
            changed.steps.as_str(),
            changed.total_timeout_ms
        ),
        ("root", "[]", 5_000)
    );
}

#[tokio::test]
async fn names_are_checked() {
    let db = Db::open_in_memory().await.expect("open");
    let first = db
        .create_login_script(&fields("sudo"))
        .await
        .expect("create");
    assert!(matches!(
        db.create_login_script(&fields("  ")).await,
        Err(LoginScriptError::NameEmpty)
    ));
    let taken = db.create_login_script(&fields("SUDO")).await;
    assert_eq!(taken.err().map(|error| error.code()), Some("name_taken"));
    let other = db.create_login_script(&fields("su")).await.expect("create");
    let renamed = db.update_login_script(other.id, &fields("Sudo")).await;
    assert_eq!(renamed.err().map(|error| error.code()), Some("name_taken"));
    db.update_login_script(first.id, &fields("sudo"))
        .await
        .expect("its own name is free");
    let missing = db
        .update_login_script(LoginScriptId(99), &fields("x"))
        .await;
    assert_eq!(
        missing.err().map(|error| error.code()),
        Some("login_script_not_found")
    );
}

#[tokio::test]
async fn a_connection_runs_a_script_until_it_is_deleted() {
    let db = Db::open_in_memory().await.expect("open");
    let script = db
        .create_login_script(&fields("sudo"))
        .await
        .expect("create");
    let connection = db
        .create_connection(None, &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("connection");
    let linked = db
        .set_connection_login_script(connection.id, Some(script.id))
        .await
        .expect("link");
    assert_eq!(linked.login_script_id, Some(script.id.0));
    assert_eq!(
        db.login_script(script.id).await.expect("get").connections,
        [connection.id]
    );
    let unknown = db
        .set_connection_login_script(connection.id, Some(LoginScriptId(42)))
        .await;
    assert_eq!(
        unknown.err().map(|error| error.code()),
        Some("login_script_not_found")
    );
    let nobody = db.set_connection_login_script(ConnectionId(77), None).await;
    assert_eq!(
        nobody.err().map(|error| error.code()),
        Some("connection_not_found")
    );

    db.store_login_script_secret(script.id, "root", |_| Ok::<_, ()>(vec![1, 2, 3]))
        .await
        .expect("store")
        .expect("sealed");
    let deletion = db.delete_login_script(script.id).await.expect("delete");
    assert_eq!(
        deletion,
        LoginScriptDeletion {
            secrets: 1,
            connections: 1
        }
    );
    let after = db.connection(connection.id).await.expect("connection");
    assert_eq!(after.login_script_id, None);
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 0);
    assert!(matches!(
        db.delete_login_script(script.id).await,
        Err(LoginScriptError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_secret_is_sealed_for_its_row_and_listed_only_by_label() {
    let db = Db::open_in_memory().await.expect("open");
    let script = db.create_login_script(&fields("su")).await.expect("create");
    let mut sealed_for = String::new();
    let secret = db
        .store_login_script_secret(script.id, " Root ", |row| {
            sealed_for = row.to_owned();
            Ok::<_, ()>(b"cipher-1".to_vec())
        })
        .await
        .expect("store")
        .expect("sealed");
    assert_eq!(secret.label, "Root");
    assert_eq!(sealed_for, secret.id.to_string());
    assert_eq!(
        db.login_script_secret(script.id, secret.id)
            .await
            .expect("read"),
        Some(b"cipher-1".to_vec())
    );
    assert_eq!(
        db.login_script(script.id).await.expect("get").secrets,
        std::slice::from_ref(&secret)
    );

    // The same label, any case, replaces the value in the same row.
    let again = db
        .store_login_script_secret(script.id, "ROOT", |_| Ok::<_, ()>(b"cipher-2".to_vec()))
        .await
        .expect("store")
        .expect("sealed");
    assert_eq!(again, secret);
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 1);

    // Another script cannot read it.
    let other = db.create_login_script(&fields("x")).await.expect("create");
    assert_eq!(
        db.login_script_secret(other.id, secret.id)
            .await
            .expect("read"),
        None
    );
    assert!(matches!(
        db.delete_login_script_secret(other.id, secret.id).await,
        Err(LoginScriptError::SecretNotFound { .. })
    ));
    db.delete_login_script_secret(script.id, secret.id)
        .await
        .expect("delete");
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 0);
}

#[tokio::test]
async fn a_failed_seal_writes_nothing() {
    let db = Db::open_in_memory().await.expect("open");
    let script = db.create_login_script(&fields("su")).await.expect("create");
    let refused = db
        .store_login_script_secret(script.id, "root", |_| Err("vault_locked"))
        .await
        .expect("no database failure");
    assert_eq!(refused, Err("vault_locked"));
    assert!(
        db.login_script(script.id)
            .await
            .expect("get")
            .secrets
            .is_empty()
    );
    assert!(matches!(
        db.store_login_script_secret(script.id, " ", |_| Ok::<_, ()>(Vec::new()))
            .await,
        Err(LoginScriptError::LabelEmpty)
    ));
    assert!(matches!(
        db.store_login_script_secret(LoginScriptId(5), "a", |_| Ok::<_, ()>(Vec::new()))
            .await,
        Err(LoginScriptError::NotFound { .. })
    ));
}

#[tokio::test]
async fn scripts_and_their_secrets_survive_a_restart() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("open");
    let script = db
        .create_login_script(&fields("sudo"))
        .await
        .expect("create");
    let secret = db
        .store_login_script_secret(script.id, "root", |_| Ok::<_, ()>(b"sealed".to_vec()))
        .await
        .expect("store")
        .expect("sealed");
    db.close().await;

    let db = Db::open(&path).await.expect("reopen");
    let again = db.login_script(script.id).await.expect("get");
    assert_eq!(again.steps, STEPS);
    assert_eq!(again.secrets, std::slice::from_ref(&secret));
    assert_eq!(
        db.login_script_secret(script.id, secret.id)
            .await
            .expect("read"),
        Some(b"sealed".to_vec())
    );
}

#[test]
fn every_code_is_listed_once() {
    let mut codes = LoginScriptError::ALL_CODES.to_vec();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), LoginScriptError::ALL_CODES.len());
    for error in [
        LoginScriptError::NotFound {
            id: LoginScriptId(1),
        },
        LoginScriptError::SecretNotFound {
            script: LoginScriptId(1),
            id: 1,
        },
        LoginScriptError::NameEmpty,
        LoginScriptError::NameTaken { name: "x".into() },
        LoginScriptError::LabelEmpty,
        LoginScriptError::ConnectionNotFound {
            id: ConnectionId(1),
        },
    ] {
        assert!(
            LoginScriptError::ALL_CODES.contains(&error.code()),
            "{error}"
        );
    }
}
