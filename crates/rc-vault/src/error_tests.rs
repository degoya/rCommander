use rc_db::DbError;

use super::VaultError;

/// One error of every variant. The match stops compiling when a variant is added: give it a
/// sample here and its code in `ALL_CODES`.
fn one_of_each() -> Vec<VaultError> {
    let samples = vec![
        VaultError::Absent,
        VaultError::Exists,
        VaultError::Locked,
        VaultError::WrongPassword,
        VaultError::EmptyPassword,
        VaultError::Malformed,
        VaultError::Undecryptable,
        VaultError::BadParameters,
        VaultError::Interrupted,
        VaultError::InUse { secrets: 1 },
        VaultError::Changed,
        VaultError::Db(DbError::NoDataDir),
    ];
    for sample in &samples {
        match sample {
            VaultError::Absent
            | VaultError::Exists
            | VaultError::Locked
            | VaultError::WrongPassword
            | VaultError::EmptyPassword
            | VaultError::Malformed
            | VaultError::Undecryptable
            | VaultError::BadParameters
            | VaultError::Interrupted
            | VaultError::InUse { .. }
            | VaultError::Changed
            | VaultError::Db(_) => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_code_once() {
    let codes: Vec<&str> = one_of_each().iter().map(VaultError::code).collect();
    assert_eq!(codes, VaultError::ALL_CODES);
}
