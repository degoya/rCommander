use secrecy::ExposeSecret as _;

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

#[tokio::test]
async fn an_export_opens_with_its_password() {
    let sealed = seal_export(&password("export pw"), CHEAP, b"{\"secret\":\"hunter2\"}")
        .await
        .expect("seals");
    assert_eq!(sealed.kdf, "argon2id");
    assert_eq!(sealed.params, CHEAP);
    assert!(!sealed.sealed.windows(7).any(|window| window == b"hunter2"));
    let opened = open_export(&password("export pw"), &sealed)
        .await
        .expect("opens");
    assert_eq!(opened.expose_secret(), b"{\"secret\":\"hunter2\"}");
}

#[tokio::test]
async fn a_wrong_password_and_a_changed_byte_fail_alike() {
    let sealed = seal_export(&password("export pw"), CHEAP, b"content")
        .await
        .expect("seals");
    let wrong = open_export(&password("other"), &sealed).await;
    assert!(matches!(wrong, Err(VaultError::WrongPassword)));
    let mut changed = sealed.clone();
    let last = changed.sealed.len() - 1;
    changed.sealed[last] ^= 1;
    let tampered = open_export(&password("export pw"), &changed).await;
    assert!(matches!(tampered, Err(VaultError::WrongPassword)));
}

#[tokio::test]
async fn hostile_parameters_are_refused_before_deriving() {
    let mut sealed = seal_export(&password("export pw"), CHEAP, b"content")
        .await
        .expect("seals");
    sealed.params.m_cost_kib = u32::MAX;
    let huge = open_export(&password("export pw"), &sealed).await;
    assert!(matches!(huge, Err(VaultError::BadParameters)));
    let mut other = sealed.clone();
    other.params = CHEAP;
    other.kdf = "scrypt".to_owned();
    assert!(matches!(
        open_export(&password("export pw"), &other).await,
        Err(VaultError::BadParameters)
    ));
    let mut short_salt = other.clone();
    short_salt.kdf = "argon2id".to_owned();
    short_salt.salt.truncate(3);
    assert!(matches!(
        open_export(&password("export pw"), &short_salt).await,
        Err(VaultError::BadParameters)
    ));
}

#[tokio::test]
async fn an_empty_export_password_is_refused() {
    let empty = seal_export(&password(""), CHEAP, b"content").await;
    assert!(matches!(empty, Err(VaultError::EmptyPassword)));
}

#[test]
fn debug_shows_no_bytes() {
    let sealed = SealedExport {
        kdf: "argon2id".to_owned(),
        params: CHEAP,
        salt: vec![7; 16],
        sealed: vec![1, 2, 3],
    };
    let shown = format!("{sealed:?}");
    assert!(shown.contains("sealed_len: 3"));
    assert!(!shown.contains("[7, 7"));
}
