use std::{sync::Arc, time::Duration};

use rc_db::{DB_FILE, Db, SecretColumn};
use sqlx::{Connection as _, SqliteConnection};
use tempfile::TempDir;

use super::*;
use crate::{ExposeSecret as _, KdfParams, ManualClock, VaultOptions, VaultState};

const COLUMN: SecretColumn = SecretColumn {
    table: "test_secrets",
    column: "secret",
    row_id: "id",
};

const FIELDS: usize = 1000;

fn password(text: &str) -> MasterPassword {
    MasterPassword::new(text.to_owned())
}

fn plaintext(index: usize) -> Vec<u8> {
    format!("secret number {index}").into_bytes()
}

struct Setup {
    dir: TempDir,
    vault: Vault,
    raw: SqliteConnection,
}

/// A vault with password `old` over a file database whose `test_secrets` holds `FIELDS`
/// encrypted values (row ids `0..FIELDS`) and one row without a value.
async fn setup() -> Setup {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("db");
    let vault = Vault::open_with(
        db,
        VaultOptions {
            clock: Arc::new(ManualClock::new()),
            kdf: KdfParams {
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            },
            failure_delay: Duration::ZERO,
            secret_columns: &[COLUMN],
        },
    )
    .await
    .expect("vault");
    vault.create(&password("old")).await.expect("create");

    let mut raw = SqliteConnection::connect(&format!("sqlite://{}", path.display()))
        .await
        .expect("second connection");
    // As every connection of `Db` does (RC-080-11 V5): pages this one frees while it fills the
    // table are cleared, so a stale copy of a value can only be the change's own.
    sqlx::query("PRAGMA secure_delete = ON")
        .execute(&mut raw)
        .await
        .expect("secure_delete");
    sqlx::query("CREATE TABLE test_secrets (id INTEGER PRIMARY KEY, secret BLOB)")
        .execute(&mut raw)
        .await
        .expect("table");
    let mut tx = raw.begin().await.expect("begin");
    for index in 0..FIELDS {
        let id = index.to_string();
        let value = vault
            .encrypt(
                &FieldRef::new("test_secrets", "secret", &id),
                &plaintext(index),
            )
            .expect("encrypt");
        sqlx::query("INSERT INTO test_secrets (id, secret) VALUES (?, ?)")
            .bind(i64::try_from(index).expect("small"))
            .bind(value)
            .execute(&mut *tx)
            .await
            .expect("insert");
    }
    sqlx::query("INSERT INTO test_secrets (id, secret) VALUES (?, NULL)")
        .bind(i64::try_from(FIELDS).expect("small"))
        .execute(&mut *tx)
        .await
        .expect("insert");
    tx.commit().await.expect("commit");
    Setup { dir, vault, raw }
}

async fn stored(raw: &mut SqliteConnection) -> Vec<(i64, Vec<u8>)> {
    sqlx::query_as("SELECT id, secret FROM test_secrets WHERE secret IS NOT NULL ORDER BY id")
        .fetch_all(raw)
        .await
        .expect("read")
}

/// Every stored value decrypts to its plaintext under the vault's key in memory.
async fn all_readable(setup: &mut Setup) -> usize {
    let rows = stored(&mut setup.raw).await;
    for (id, value) in &rows {
        let row_id = id.to_string();
        let plain = setup
            .vault
            .decrypt(&FieldRef::new("test_secrets", "secret", &row_id), value)
            .expect("decrypts");
        let index = usize::try_from(*id).expect("small");
        assert_eq!(plain.expose_secret(), plaintext(index).as_slice());
    }
    rows.len()
}

#[tokio::test]
async fn a_failure_after_field_500_leaves_everything_under_the_old_password() {
    let mut setup = setup().await;
    let before = stored(&mut setup.raw).await;
    *setup
        .vault
        .shared
        .fail_rekey_after
        .lock()
        .expect("not poisoned") = Some(500);

    let result = setup
        .vault
        .change_password(&password("old"), &password("new"))
        .await;
    assert!(matches!(result, Err(VaultError::Interrupted)), "{result:?}");
    assert_eq!(stored(&mut setup.raw).await, before, "no value changed");

    // The key in memory is still the old one, and so is the password on disk.
    assert_eq!(all_readable(&mut setup).await, FIELDS);
    setup.vault.lock();
    assert!(matches!(
        setup.vault.unlock(&password("new")).await,
        Err(VaultError::WrongPassword)
    ));
    setup
        .vault
        .unlock(&password("old"))
        .await
        .expect("old unlocks");
    assert_eq!(all_readable(&mut setup).await, FIELDS);
}

#[tokio::test]
async fn a_password_change_re_encrypts_all_1000_fields() {
    let mut setup = setup().await;
    let before = stored(&mut setup.raw).await;

    let count = setup
        .vault
        .change_password(&password("old"), &password("new"))
        .await
        .expect("change");
    assert_eq!(count, FIELDS);
    let after = stored(&mut setup.raw).await;
    assert_eq!(after.len(), FIELDS);
    assert!(
        before.iter().zip(&after).all(|(old, new)| old.1 != new.1),
        "every value was rewritten"
    );
    assert!(matches!(setup.vault.state(), VaultState::Unlocked { .. }));
    assert_eq!(all_readable(&mut setup).await, FIELDS);

    setup.vault.lock();
    assert!(matches!(
        setup.vault.unlock(&password("old")).await,
        Err(VaultError::WrongPassword)
    ));
    setup
        .vault
        .unlock(&password("new"))
        .await
        .expect("new unlocks");
    assert_eq!(all_readable(&mut setup).await, FIELDS);
}

#[tokio::test]
async fn the_current_password_is_checked_first() {
    let mut setup = setup().await;
    let before = stored(&mut setup.raw).await;
    assert!(matches!(
        setup
            .vault
            .change_password(&password("wrong"), &password("new"))
            .await,
        Err(VaultError::WrongPassword)
    ));
    assert!(matches!(
        setup
            .vault
            .change_password(&password("old"), &password(""))
            .await,
        Err(VaultError::EmptyPassword)
    ));
    assert_eq!(stored(&mut setup.raw).await, before);
    assert_eq!(all_readable(&mut setup).await, FIELDS);
}

#[tokio::test]
async fn a_value_the_old_key_cannot_open_stops_the_change() {
    let mut setup = setup().await;
    sqlx::query("UPDATE test_secrets SET secret = ? WHERE id = 3")
        .bind(vec![1_u8; 60])
        .execute(&mut setup.raw)
        .await
        .expect("damage one value");
    let before = stored(&mut setup.raw).await;
    assert!(matches!(
        setup
            .vault
            .change_password(&password("old"), &password("new"))
            .await,
        Err(VaultError::Undecryptable)
    ));
    assert_eq!(stored(&mut setup.raw).await, before);
    setup.vault.lock();
    setup
        .vault
        .unlock(&password("old"))
        .await
        .expect("old unlocks");
}

/// Whether `needle` occurs in `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// RC-080-11 (V5): after a password change no old ciphertext, salt or verifier is left in the
/// database file or its WAL - with the old password they would still open.
#[tokio::test]
async fn after_the_change_no_old_ciphertext_is_left_in_the_file() {
    let mut setup = setup().await;
    let before = stored(&mut setup.raw).await;
    let (salt, verifier): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT salt, verifier FROM vault_meta WHERE id = 1")
            .fetch_one(&mut setup.raw)
            .await
            .expect("the vault row");
    setup
        .vault
        .change_password(&password("old"), &password("new"))
        .await
        .expect("change");

    let path = setup.dir.path().join(DB_FILE);
    let mut bytes = std::fs::read(&path).expect("the database file");
    let wal = path.with_file_name(format!("{DB_FILE}-wal"));
    if let Ok(log) = std::fs::read(&wal) {
        assert!(log.is_empty(), "the WAL was truncated");
        bytes.extend(log);
    }
    let after = stored(&mut setup.raw).await;
    assert!(
        contains(&bytes, &after[0].1),
        "the probe finds a current value"
    );
    for (_, old) in before.iter().step_by(50) {
        assert!(!contains(&bytes, old), "an old ciphertext is left");
    }
    assert!(!contains(&bytes, &salt), "the old salt is left");
    assert!(!contains(&bytes, &verifier), "the old verifier is left");
}
