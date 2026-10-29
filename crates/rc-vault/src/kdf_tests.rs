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
        KdfParams::from_meta(&meta(KDF_NAME, 64, 1, 1)).expect("valid"),
        CHEAP
    );
    assert_eq!(
        KdfParams::from_meta(&meta(KDF_NAME, 65_536, 3, 1)).expect("valid"),
        KdfParams::DEFAULT
    );
    assert!(KdfParams::from_meta(&meta(KDF_NAME, 1024 * 1024, 16, 16)).is_ok());
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
            matches!(KdfParams::from_meta(&bad), Err(VaultError::BadParameters)),
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
