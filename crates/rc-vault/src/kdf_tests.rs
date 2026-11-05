use super::*;

/// Parameters that derive in milliseconds.
const CHEAP: KdfParams = KdfParams {
    m_cost_kib: 64,
    t_cost: 1,
    p_cost: 1,
};

fn password(text: &str) -> MasterPassword {
    MasterPassword::new(text.to_owned())
}

fn derive(text: &str, salt: &[u8], params: KdfParams) -> Vec<u8> {
    derive_key(&password(text), salt, params)
        .expect("derives")
        .bytes()
        .to_vec()
}

fn meta(kdf: &str, m: i64, t: i64, p: i64) -> VaultMeta {
    VaultMeta {
        version: 1,
        kdf: kdf.to_owned(),
        m_cost_kib: m,
        t_cost: t,
        p_cost: p,
        salt: vec![0; SALT_LEN],
        verifier: Vec::new(),
    }
}

#[test]
fn the_same_input_derives_the_same_key() {
    let salt = [7_u8; SALT_LEN];
    let key = derive("correct horse", &salt, CHEAP);
    assert_eq!(key.len(), KEY_LEN);
    assert_eq!(key, derive("correct horse", &salt, CHEAP));
}

#[test]
fn password_salt_and_parameters_each_change_the_key() {
    let salt = [7_u8; SALT_LEN];
    let key = derive("correct horse", &salt, CHEAP);
    assert_ne!(key, derive("correct horsf", &salt, CHEAP));
    assert_ne!(key, derive("correct horse", &[8_u8; SALT_LEN], CHEAP));
    let more = KdfParams { t_cost: 2, ..CHEAP };
    assert_ne!(key, derive("correct horse", &salt, more));
}

#[test]
fn new_salts_differ() {
    assert_ne!(new_salt(), new_salt());
}

#[test]
fn stored_parameters_are_read_and_checked() {
    assert_eq!(
        KdfParams::from_meta(&meta(KDF_NAME, 64, 1, 1), CHEAP).expect("valid"),
        CHEAP
    );
    assert_eq!(
        KdfParams::from_meta(&meta(KDF_NAME, 65_536, 3, 1), CHEAP).expect("valid"),
        KdfParams::DEFAULT
    );
    assert!(KdfParams::from_meta(&meta(KDF_NAME, 1024 * 1024, 16, 16), CHEAP).is_ok());
    let mut newer = meta(KDF_NAME, 64, 1, 1);
    newer.version = 2;
    for bad in [
        newer,
        meta("scrypt", 64, 1, 1),
        meta(KDF_NAME, 1024 * 1024 + 1, 1, 1),
        meta(KDF_NAME, 64, 17, 1),
        meta(KDF_NAME, -1, 1, 1),
        meta(KDF_NAME, 64, i64::from(u32::MAX) + 1, 1),
        meta(KDF_NAME, 8 * 1024 * 1024, 1, 1),
        meta(KDF_NAME, 64, 1000, 1),
        meta(KDF_NAME, 64, 1, 64),
    ] {
        assert!(
            matches!(
                KdfParams::from_meta(&bad, CHEAP),
                Err(VaultError::BadParameters)
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn parameters_argon2_refuses_are_an_error_not_a_panic() {
    let too_little = KdfParams {
        m_cost_kib: 1,
        ..CHEAP
    };
    assert!(matches!(
        derive_key(&password("x"), &[0; SALT_LEN], too_little),
        Err(VaultError::BadParameters)
    ));
    assert!(matches!(
        derive_key(&password("x"), b"short", CHEAP),
        Err(VaultError::BadParameters)
    ));
}

#[test]
fn a_dropped_key_is_zeroed_and_never_printed() {
    let before = wipe_hook::wiped();
    let key = VaultKey::from_bytes([0x5A; KEY_LEN]);
    assert_eq!(format!("{key:?}"), "VaultKey([REDACTED])");
    drop(key);
    assert_eq!(wipe_hook::wiped(), before + 1);
}

/// RC-080-42: a release build reads and makes nothing below the floor - OWASP's 19 MiB and two
/// passes - and the default lies above it; the tests' cheap parameters pass in a debug build only.
#[test]
fn the_floor_refuses_parameters_cheap_to_guess_against() {
    let floor = KdfParams::MIN;
    assert_eq!(
        KdfParams::DEFAULT.within(floor, KdfParams::MAX).ok(),
        Some(KdfParams::DEFAULT)
    );
    assert_eq!(floor.within(floor, KdfParams::MAX).ok(), Some(floor));
    for weak in [
        CHEAP,
        KdfParams {
            m_cost_kib: floor.m_cost_kib - 1,
            ..floor
        },
        KdfParams { t_cost: 1, ..floor },
        KdfParams { p_cost: 0, ..floor },
    ] {
        assert!(
            matches!(
                weak.within(floor, KdfParams::MAX),
                Err(VaultError::BadParameters)
            ),
            "{weak:?}"
        );
    }
    assert_eq!(CHEAP.checked().is_ok(), cfg!(debug_assertions));
}

/// RC-080-68: what is read is held to the release floor whatever the build - a debug build of the
/// program, which makes its keys with the default, refuses a weak stored row and a weak export;
/// only a process that makes keys as weak itself (the tests of every crate) reads them.
#[test]
fn reading_holds_the_release_floor_in_every_build() {
    let weak = meta(KDF_NAME, 64, 1, 1);
    assert!(matches!(
        KdfParams::from_meta(&weak, KdfParams::DEFAULT),
        Err(VaultError::BadParameters)
    ));
    assert_eq!(KdfParams::read_floor(KdfParams::DEFAULT), KdfParams::MIN);
    assert_eq!(KdfParams::read_floor(KdfParams::MIN), KdfParams::MIN);
    assert!(matches!(
        CHEAP.checked_import(KdfParams::DEFAULT),
        Err(VaultError::BadParameters)
    ));
    assert_eq!(
        KdfParams::MIN.checked_import(KdfParams::DEFAULT).ok(),
        Some(KdfParams::MIN)
    );
    // The tests' own weak parameters are read only by a process that makes keys with them, and
    // only in a debug build.
    assert_eq!(
        KdfParams::from_meta(&weak, CHEAP).is_ok(),
        cfg!(debug_assertions)
    );
    // Never below argon2's own least, whatever the own parameters say.
    let none = KdfParams {
        m_cost_kib: 0,
        t_cost: 0,
        p_cost: 0,
    };
    assert_eq!(
        KdfParams::read_floor(none),
        if cfg!(debug_assertions) {
            KdfParams::ARGON2_LEAST
        } else {
            KdfParams::MIN
        }
    );
}

/// RC-080-68: an export from elsewhere is capped below the own database - 256 MiB, eight passes,
/// four lanes - and refused above it as too costly, not as damaged.
#[test]
fn an_import_is_capped_below_the_own_database() {
    let cap = KdfParams::IMPORT_MAX;
    assert_eq!(cap.checked_import(CHEAP).ok(), Some(cap));
    assert_eq!(
        KdfParams::DEFAULT.checked_import(KdfParams::DEFAULT).ok(),
        Some(KdfParams::DEFAULT)
    );
    for costly in [
        KdfParams {
            m_cost_kib: 1024 * 1024,
            ..CHEAP
        },
        KdfParams {
            m_cost_kib: cap.m_cost_kib + 1,
            ..CHEAP
        },
        KdfParams { t_cost: 9, ..CHEAP },
        KdfParams { p_cost: 5, ..CHEAP },
    ] {
        assert!(
            matches!(costly.checked_import(CHEAP), Err(VaultError::TooCostly)),
            "{costly:?}"
        );
    }
    // The own database keeps its higher cap.
    assert!(KdfParams::from_meta(&meta(KDF_NAME, 1024 * 1024, 16, 16), CHEAP).is_ok());
}
