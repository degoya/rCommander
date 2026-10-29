use std::time::{Duration, Instant};

use super::*;
use crate::{ExposeSecret as _, ManualClock, VaultStateKind, kdf::wipe_hook};

/// Parameters that derive in milliseconds.
const CHEAP: KdfParams = KdfParams {
    m_cost_kib: 64,
    t_cost: 1,
    p_cost: 1,
};

fn options(clock: &ManualClock) -> VaultOptions {
    VaultOptions {
        clock: Arc::new(clock.clone()),
        kdf: CHEAP,
        failure_delay: Duration::ZERO,
        secret_columns: &[],
    }
}

fn password(text: &str) -> MasterPassword {
    MasterPassword::new(text.to_owned())
}

async fn new_vault(clock: &ManualClock) -> Vault {
    let db = Db::open_in_memory().await.expect("db");
    Vault::open_with(db, options(clock)).await.expect("vault")
}

const FIELD: FieldRef<'static> = FieldRef {
    table: "connections",
    column: "password",
    row_id: "7",
};

#[tokio::test]
async fn without_a_row_there_is_no_vault_and_nothing_to_unlock() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    assert_eq!(vault.state(), VaultState::Absent);
    assert_eq!(vault.status().state, VaultStateKind::Absent);
    assert!(matches!(
        vault.unlock(&password("x")).await,
        Err(VaultError::Absent)
    ));
    assert!(matches!(
        vault.encrypt(&FIELD, b"x"),
        Err(VaultError::Absent)
    ));
    assert!(matches!(vault.touch(), Err(VaultError::Absent)));
    assert!(matches!(
        vault.create(&password("")).await,
        Err(VaultError::EmptyPassword)
    ));
    assert_eq!(vault.state(), VaultState::Absent);
}

#[tokio::test]
async fn create_then_unlock_with_the_right_password_decrypts() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault
        .create(&password("correct horse"))
        .await
        .expect("create");
    assert_eq!(vault.status().state, VaultStateKind::Unlocked);
    let stored = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");
    assert!(matches!(
        vault.create(&password("again")).await,
        Err(VaultError::Exists)
    ));

    assert!(vault.lock());
    vault
        .unlock(&password("correct horse"))
        .await
        .expect("unlock");
    let plaintext = vault.decrypt(&FIELD, &stored).expect("decrypt");
    assert_eq!(plaintext.expose_secret(), b"hunter2");
}

#[tokio::test]
async fn a_wrong_password_is_wrong_password_and_stays_locked() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault
        .create(&password("correct horse"))
        .await
        .expect("create");
    vault.lock();
    for wrong in ["correct horsf", "", "Correct horse", "correct horse "] {
        assert!(matches!(
            vault.unlock(&password(wrong)).await,
            Err(VaultError::WrongPassword)
        ));
        assert_eq!(vault.state(), VaultState::Locked);
    }
}

#[tokio::test]
async fn reopening_the_database_finds_the_vault_locked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(rc_db::DB_FILE);
    let clock = ManualClock::new();
    let stored = {
        let db = Db::open(&path).await.expect("db");
        let vault = Vault::open_with(db.clone(), options(&clock))
            .await
            .expect("vault");
        vault.create(&password("pw")).await.expect("create");
        let stored = vault.encrypt(&FIELD, b"secret").expect("encrypt");
        drop(vault);
        db.close().await;
        stored
    };
    let vault = Vault::open_with(Db::open(&path).await.expect("db"), options(&clock))
        .await
        .expect("vault");
    assert_eq!(vault.state(), VaultState::Locked);
    vault.unlock(&password("pw")).await.expect("unlock");
    assert_eq!(
        vault
            .decrypt(&FIELD, &stored)
            .expect("decrypt")
            .expose_secret(),
        b"secret"
    );
}

#[tokio::test]
async fn a_value_moved_to_another_row_or_column_does_not_decrypt() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault.create(&password("pw")).await.expect("create");
    let stored = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");
    for elsewhere in [
        FieldRef::new("connections", "password", "8"),
        FieldRef::new("connections", "passphrase", "7"),
        FieldRef::new("keys", "password", "7"),
        FieldRef::new("connections", "password", "7/"),
    ] {
        assert!(
            matches!(
                vault.decrypt(&elsewhere, &stored),
                Err(VaultError::Undecryptable)
            ),
            "{elsewhere:?}"
        );
    }
    assert!(vault.decrypt(&FIELD, &stored).is_ok());
}

#[tokio::test]
async fn the_same_plaintext_encrypts_differently() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault.create(&password("pw")).await.expect("create");
    let first = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");
    let second = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");
    assert_ne!(first, second);
}

#[tokio::test]
async fn after_lock_every_decryption_is_locked_and_the_key_was_zeroed() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault.create(&password("pw")).await.expect("create");
    let stored = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");

    let wiped = wipe_hook::wiped();
    assert!(vault.lock());
    assert_eq!(wipe_hook::wiped(), wiped + 1, "the key was dropped zeroed");
    assert!(matches!(
        vault.decrypt(&FIELD, &stored),
        Err(VaultError::Locked)
    ));
    assert!(matches!(
        vault.decrypt_string(&FIELD, &stored),
        Err(VaultError::Locked)
    ));
    assert!(matches!(
        vault.encrypt(&FIELD, b"x"),
        Err(VaultError::Locked)
    ));
    assert!(!vault.lock(), "nothing left to drop");
    assert_eq!(wipe_hook::wiped(), wiped + 1);
}

#[tokio::test]
async fn the_idle_lock_falls_due_after_the_setting_and_use_defers_it() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    assert_eq!(vault.idle_lock_minutes(), DEFAULT_IDLE_LOCK_MINUTES);
    vault.set_idle_lock_minutes(5).await.expect("setting");
    vault.create(&password("pw")).await.expect("create");
    let stored = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");

    clock.advance(Duration::from_secs(4 * 60));
    assert!(!vault.lock_if_idle());
    assert_eq!(vault.idle_lock_in(), Some(Duration::from_secs(60)));
    vault.decrypt(&FIELD, &stored).expect("a decryption is use");
    clock.advance(Duration::from_secs(4 * 60 + 59));
    assert!(!vault.lock_if_idle());

    clock.advance(Duration::from_secs(1));
    assert!(vault.lock_if_idle());
    assert_eq!(vault.state(), VaultState::Locked);
    assert_eq!(vault.idle_lock_in(), None);
}

#[tokio::test]
async fn the_idle_lock_setting_is_stored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(rc_db::DB_FILE);
    let clock = ManualClock::new();
    let db = Db::open(&path).await.expect("db");
    let vault = Vault::open_with(db.clone(), options(&clock))
        .await
        .expect("vault");
    vault.set_idle_lock_minutes(0).await.expect("setting");
    vault.set_max_unlocked_hours(0).await.expect("setting");
    drop(vault);
    let vault = Vault::open_with(db, options(&clock)).await.expect("vault");
    assert_eq!(vault.idle_lock_minutes(), 0);
    vault.create(&password("pw")).await.expect("create");
    clock.advance(Duration::from_secs(365 * 24 * 3600));
    assert!(!vault.lock_if_idle());
    assert_eq!(vault.status().state, VaultStateKind::Unlocked);
}

/// RC-080-11 (V1): an agent's read opens with the key and leaves the idle lock where it was;
/// the longest time unlocked (default eight hours, stored like the idle lock) locks however
/// often the person reads.
#[tokio::test]
async fn an_agents_read_does_not_defer_and_the_longest_time_locks() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault.create(&password("pw")).await.expect("create");
    let stored = vault
        .encrypt_str(&FIELD, &SecretString::from("hunter2"))
        .expect("encrypt");
    clock.advance(Duration::from_secs(10 * 60));
    vault
        .decrypt_string_for(&FIELD, &stored, Use::Agent)
        .expect("the key opens for an agent");
    assert_eq!(vault.idle_lock_in(), Some(Duration::from_secs(5 * 60)));
    vault
        .decrypt_string_for(&FIELD, &stored, Use::Person)
        .expect("the person's read");
    assert_eq!(vault.idle_lock_in(), Some(Duration::from_secs(15 * 60)));

    for _ in 0..46 {
        clock.advance(Duration::from_secs(10 * 60));
        vault.decrypt(&FIELD, &stored).expect("still unlocked");
    }
    // Eight hours from the unlock, however busy.
    clock.advance(Duration::from_secs(10 * 60));
    assert!(matches!(
        vault.decrypt(&FIELD, &stored),
        Err(VaultError::Locked)
    ));

    let db = vault.shared.db.clone();
    vault.set_max_unlocked_hours(1).await.expect("setting");
    let reopened = Vault::open_with(db, options(&clock)).await.expect("vault");
    reopened.unlock(&password("pw")).await.expect("unlock");
    assert_eq!(reopened.idle_lock_in(), Some(Duration::from_secs(15 * 60)));
    reopened.set_idle_lock_minutes(0).await.expect("setting");
    assert_eq!(reopened.idle_lock_in(), Some(Duration::from_secs(3600)));
}

#[tokio::test]
async fn kdf_parameters_are_read_from_the_vault_not_assumed() {
    let clock = ManualClock::new();
    let db = Db::open_in_memory().await.expect("db");
    let made_with = KdfParams {
        m_cost_kib: 128,
        t_cost: 2,
        p_cost: 2,
    };
    let vault = Vault::open_with(
        db.clone(),
        VaultOptions {
            kdf: made_with,
            ..options(&clock)
        },
    )
    .await
    .expect("vault");
    vault.create(&password("pw")).await.expect("create");
    let stored = vault.encrypt(&FIELD, b"hunter2").expect("encrypt");
    let meta = db.vault_meta().await.expect("read").expect("a row");
    assert_eq!(
        (meta.m_cost_kib, meta.t_cost, meta.p_cost),
        (128, 2, 2),
        "the parameters are stored"
    );
    drop(vault);

    // A build whose new vaults use other parameters opens this one with the stored ones.
    let vault = Vault::open_with(db, options(&clock)).await.expect("vault");
    vault.unlock(&password("pw")).await.expect("unlock");
    assert_eq!(
        vault
            .decrypt(&FIELD, &stored)
            .expect("decrypt")
            .expose_secret(),
        b"hunter2"
    );
}

#[tokio::test]
async fn repeated_failures_slow_the_next_attempt_down() {
    let clock = ManualClock::new();
    let delay = Duration::from_millis(40);
    let db = Db::open_in_memory().await.expect("db");
    let vault = Vault::open_with(
        db,
        VaultOptions {
            failure_delay: delay,
            ..options(&clock)
        },
    )
    .await
    .expect("vault");
    vault.create(&password("pw")).await.expect("create");
    vault.lock();
    for _ in 0..3 {
        assert!(vault.unlock(&password("no")).await.is_err());
    }
    let started = Instant::now();
    assert!(matches!(vault.unlock(&password("pw")).await, Ok(())));
    assert!(started.elapsed() >= delay, "the fourth attempt waited");

    // Success starts the count over.
    vault.lock();
    let started = Instant::now();
    assert!(vault.unlock(&password("no")).await.is_err());
    assert!(started.elapsed() < delay * 10);
}

#[tokio::test]
async fn text_secrets_round_trip() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    vault.create(&password("pw")).await.expect("create");
    let secret = SecretString::from("p\u{e4}ssw\u{f6}rd".to_owned());
    let stored = vault.encrypt_str(&FIELD, &secret).expect("encrypt");
    let back = vault.decrypt_string(&FIELD, &stored).expect("decrypt");
    assert_eq!(back.expose_secret(), secret.expose_secret());

    let bytes = vault.encrypt(&FIELD, &[0xFF, 0xFE]).expect("encrypt");
    assert!(matches!(
        vault.decrypt_string(&FIELD, &bytes),
        Err(VaultError::Malformed)
    ));
}

#[tokio::test]
async fn debug_output_never_shows_a_secret() {
    let clock = ManualClock::new();
    let vault = new_vault(&clock).await;
    let master = password("correct horse battery");
    vault.create(&master).await.expect("create");
    let stored = vault.encrypt(&FIELD, b"hunter2-plain").expect("encrypt");
    let text = vault.decrypt_string(&FIELD, &stored).expect("decrypt");
    let bytes = vault.decrypt(&FIELD, &stored).expect("decrypt");

    let printed = [
        format!("{master:?}"),
        format!("{text:?}"),
        format!("{bytes:?}"),
        format!("{:?}", vault.state()),
        format!("{:?}", vault.status()),
        format!("{:?}", VaultError::WrongPassword),
        format!("{}", VaultError::WrongPassword),
    ];
    for line in printed {
        assert!(!line.contains("hunter2"), "{line}");
        assert!(!line.contains("correct horse"), "{line}");
    }
}

#[tokio::test]
async fn an_empty_vault_is_removed_and_asks_nothing_any_more() {
    let clock = ManualClock::new();
    let db = Db::open_in_memory().await.expect("db");
    let vault = Vault::open_with(
        db.clone(),
        VaultOptions {
            secret_columns: SECRET_COLUMNS,
            ..options(&clock)
        },
    )
    .await
    .expect("vault");
    assert!(matches!(vault.remove().await, Err(VaultError::Absent)));
    vault.create(&password("pw")).await.expect("create");

    let id = db
        .create_connection(None, &rc_db::connections::ConnectionDraft::new("web", "h"))
        .await
        .expect("connection")
        .id;
    let row = id.to_string();
    let field = FieldRef::new("connections", "password_enc", &row);
    let cipher = vault.encrypt(&field, b"secret").expect("encrypt");
    db.set_connection_secret(id, rc_db::SecretKind::Password, Some(cipher.as_slice()))
        .await
        .expect("store");
    assert!(matches!(
        vault.remove().await,
        Err(VaultError::InUse { secrets: 1 })
    ));
    assert!(matches!(vault.state(), VaultState::Unlocked { .. }));

    db.set_connection_secret(id, rc_db::SecretKind::Password, None)
        .await
        .expect("clear");
    vault.remove().await.expect("remove");
    assert_eq!(vault.state(), VaultState::Absent);
    assert_eq!(db.vault_meta().await.expect("read"), None);
    assert!(matches!(
        vault.encrypt(&field, b"x"),
        Err(VaultError::Absent)
    ));
    let reopened = Vault::open_with(db, options(&clock)).await.expect("reopen");
    assert_eq!(reopened.state(), VaultState::Absent);
}
