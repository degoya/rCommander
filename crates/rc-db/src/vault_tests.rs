use super::*;

const SECRETS: SecretColumn = SecretColumn {
    table: "test_secrets",
    column: "secret",
    row_id: "id",
};

fn meta(salt: u8) -> VaultMeta {
    VaultMeta {
        version: 1,
        kdf: "argon2id".to_owned(),
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
        salt: vec![salt; 16],
        verifier: vec![0xAB; 40],
    }
}

/// A database with `test_secrets` rows `a`, `b` (values `[1]`, `[2]`) and `c` (no value).
async fn with_secrets() -> Db {
    let db = Db::open_in_memory().await.expect("open");
    {
        let mut writer = db.writer().await;
        for statement in [
            "CREATE TABLE test_secrets (id TEXT NOT NULL UNIQUE, secret BLOB)",
            "INSERT INTO test_secrets (id, secret) VALUES ('a', x'01'), ('b', x'02'), ('c', NULL)",
        ] {
            sqlx::query(statement)
                .execute(&mut *writer)
                .await
                .expect("setup");
        }
    }
    db
}

async fn values(db: &Db) -> Vec<(String, Option<Vec<u8>>)> {
    sqlx::query_as("SELECT id, secret FROM test_secrets ORDER BY id")
        .fetch_all(db.readers())
        .await
        .expect("values")
}

#[tokio::test]
async fn no_row_means_no_vault_and_the_first_insert_wins() {
    let db = Db::open_in_memory().await.expect("open");
    assert_eq!(db.vault_meta().await.expect("read"), None);
    assert!(db.insert_vault_meta(&meta(1)).await.expect("insert"));
    assert!(!db.insert_vault_meta(&meta(2)).await.expect("second insert"));
    assert_eq!(db.vault_meta().await.expect("read"), Some(meta(1)));
}

#[tokio::test]
async fn rekey_rewrites_every_value_and_the_row_in_one_go() {
    let db = with_secrets().await;
    db.insert_vault_meta(&meta(1)).await.expect("insert");
    let mut seen = Vec::new();
    let count = db
        .rekey_secrets(&[SECRETS], &meta(2), |column, id, value| {
            seen.push((column.table, id.to_owned()));
            Ok::<_, ()>(vec![value[0] + 10])
        })
        .await
        .expect("rekey");
    assert_eq!(count, 2);
    assert_eq!(
        seen,
        [
            ("test_secrets", "a".to_owned()),
            ("test_secrets", "b".to_owned())
        ]
    );
    assert_eq!(
        values(&db).await,
        [
            ("a".to_owned(), Some(vec![11])),
            ("b".to_owned(), Some(vec![12])),
            ("c".to_owned(), None),
        ]
    );
    assert_eq!(db.vault_meta().await.expect("read"), Some(meta(2)));
}

#[tokio::test]
async fn a_failing_rewrite_changes_nothing() {
    let db = with_secrets().await;
    db.insert_vault_meta(&meta(1)).await.expect("insert");
    let before = values(&db).await;
    let result = db
        .rekey_secrets(&[SECRETS], &meta(2), |_, id, value| {
            if id == "b" {
                Err("refused")
            } else {
                Ok(vec![value[0] + 10])
            }
        })
        .await;
    assert!(matches!(result, Err(RekeyError::Rewrite("refused"))));
    assert_eq!(values(&db).await, before);
    assert_eq!(db.vault_meta().await.expect("read"), Some(meta(1)));
    // The writer is free again: the rolled-back transaction did not leak.
    db.set("after", &1).await.expect("write after the rollback");
}

#[tokio::test]
async fn without_a_vault_row_the_rekey_rolls_back() {
    let db = with_secrets().await;
    let before = values(&db).await;
    let result = db
        .rekey_secrets(&[SECRETS], &meta(2), |_, _, value| {
            Ok::<_, ()>(value.to_vec())
        })
        .await;
    assert!(matches!(result, Err(RekeyError::Db(DbError::NoVault))));
    assert_eq!(values(&db).await, before);
}

#[tokio::test]
async fn names_are_refused_unless_plain_identifiers() {
    let db = with_secrets().await;
    db.insert_vault_meta(&meta(1)).await.expect("insert");
    let sneaky = SecretColumn {
        table: "test_secrets\"; DROP TABLE settings; --",
        ..SECRETS
    };
    let result = db
        .rekey_secrets(&[sneaky], &meta(2), |_, _, value| {
            Ok::<_, ()>(value.to_vec())
        })
        .await;
    assert!(matches!(
        result,
        Err(RekeyError::Db(DbError::BadIdentifier { .. }))
    ));
    assert_eq!(db.vault_meta().await.expect("read"), Some(meta(1)));
}

#[test]
fn identifiers() {
    assert!(checked_identifier("connections").is_ok());
    assert!(checked_identifier("_x1").is_ok());
    for bad in ["", "1table", "a-b", "a b", "a\"b", "t\u{e4}"] {
        assert!(checked_identifier(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_secret_columns_are_plain_identifiers() {
    assert_eq!(SECRET_COLUMNS.len(), 7);
    for column in SECRET_COLUMNS {
        for name in [column.table, column.column, column.row_id] {
            assert!(checked_identifier(name).is_ok(), "{name:?}");
        }
    }
}

#[tokio::test]
async fn the_vault_row_goes_only_when_no_secret_is_left() {
    let db = with_secrets().await;
    assert_eq!(
        db.remove_vault_meta(&[SECRETS]).await.expect("remove"),
        VaultRemoval::Absent
    );
    db.insert_vault_meta(&meta(1)).await.expect("insert");
    assert_eq!(db.stored_secrets(&[SECRETS]).await.expect("count"), 2);
    assert_eq!(
        db.remove_vault_meta(&[SECRETS]).await.expect("remove"),
        VaultRemoval::InUse { secrets: 2 }
    );
    assert_eq!(db.vault_meta().await.expect("read"), Some(meta(1)));

    sqlx::query("UPDATE test_secrets SET secret = NULL")
        .execute(&mut *db.writer().await)
        .await
        .expect("clear");
    assert_eq!(db.stored_secrets(&[SECRETS]).await.expect("count"), 0);
    assert_eq!(
        db.remove_vault_meta(&[SECRETS]).await.expect("remove"),
        VaultRemoval::Removed
    );
    assert_eq!(db.vault_meta().await.expect("read"), None);
}
